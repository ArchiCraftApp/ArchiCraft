#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "requests>=2.31",
#     "beautifulsoup4>=4.12",
#     "markdownify>=1.1",
# ]
# ///
"""Scrape units of the Graphisoft Archicad Knowledge Base into an agent-friendly corpus.

The Knowledge Base (https://community.graphisoft.com/t5/Knowledge-Base/ct-p/tkb) is a
Khoros community. Its HTML pages sit behind a Cloudflare challenge, but the public
Community API v2 is readable anonymously, so articles are read from the API and only
media files are fetched directly.

Output layout (default: archicad-kb/ next to this script):

    README.md                   how the corpus is organised (written for agents)
    index.md                    every article grouped by unit, with a one-line summary
    manifest.json               machine-readable metadata for every article
    <unit>/<slug>-<id>/
        article.md              YAML front matter + Markdown body
        media/                  images and Graphisoft-hosted videos used by article.md
        attachments/            files attached to the article (e.g. .gsm library parts)

Usage:
    uv run scrape_archicad_kb.py                        # the four default units
    uv run scrape_archicad_kb.py --units Modeling BIMx  # unit titles or board ids
    uv run scrape_archicad_kb.py --limit 3 --out /tmp/kb-test
    uv run scrape_archicad_kb.py --list-units

Re-running is incremental: text is regenerated from the API, media already on disk is
not downloaded again, and units not named in the run are kept in the index.
"""

from __future__ import annotations

import argparse
import glob
import hashlib
import json
import mimetypes
import posixpath
import re
import sys
import threading
import time
import unicodedata
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import parse_qs, unquote, urljoin, urlparse

import requests
from bs4 import BeautifulSoup, Tag
from markdownify import MarkdownConverter

BASE = "https://community.graphisoft.com"
API = f"{BASE}/api/2.0/search"
KB_URL = f"{BASE}/t5/Knowledge-Base/ct-p/tkb"
KB_CATEGORY = "tkb"
DEFAULT_UNITS = ["tkb-getting-started", "tkb_design", "tkb_document", "tkb_visualize"]
USER_AGENT = "archicad-kb-scraper/1.0 (offline reference corpus for coding agents)"

ARTICLE_FIELDS = [
    "id", "subject", "body", "view_href", "post_time", "current_revision",
    "metrics", "author", "labels", "tags", "attachments",
]
ARTICLE_SUBQUERIES = {
    "labels": {"fields": ["text"]},
    "tags": {"fields": ["text"]},
    "attachments": {"fields": ["filename", "url", "filesize", "content_type"]},
}

KHOROS_IMAGE = re.compile(r"/t5/image/serverpage/image-id/((\d+)i[0-9A-Fa-f]+)")
TOPIC_LINK = re.compile(r"/ta-p/(\d+)")
LEGACY_ICON = re.compile(r"/legacyfs/online/[^/]+\.(?:png|gif|svg)$", re.I)  # info-circle.png etc.
FILENAME_LIKE = re.compile(
    r"^(?:.*\.(?:png|jpe?g|gif|bmp|webp|tiff?|svg)|image\s*\d*|img[\s_-]*\d+.*|"
    r"screen\s*shot.*|capture.*|untitled.*|\d+)$",
    re.I,
)
EXTENSIONS = {
    "image/jpeg": ".jpg", "image/png": ".png", "image/gif": ".gif", "image/webp": ".webp",
    "image/svg+xml": ".svg", "image/bmp": ".bmp", "video/mp4": ".mp4", "video/webm": ".webm",
    "video/quicktime": ".mov",
}
HEADINGS = ["h1", "h2", "h3", "h4", "h5", "h6"]

print_lock = threading.Lock()


def log(msg: str) -> None:
    with print_lock:
        print(msg, flush=True)


def slugify(text: str, max_len: int = 60) -> str:
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    text = re.sub(r"[^a-zA-Z0-9]+", "-", text).strip("-").lower()
    if len(text) > max_len:  # cut at a word boundary
        text = text[:max_len + 1].rsplit("-", 1)[0]
    return text or "untitled"


def normalize_unit(name: str) -> str:
    return re.sub(r"[^a-z0-9]", "", name.lower().replace("isation", "ization"))


# --------------------------------------------------------------------------- HTTP


class Throttle:
    """Request pacing shared by all threads; slows down whenever the server answers 429."""

    def __init__(self, interval: float):
        self.base = self.interval = interval
        self.lock = threading.Lock()
        self.next_at = 0.0

    def wait(self) -> None:
        with self.lock:
            now = time.monotonic()
            at = max(now, self.next_at)
            self.next_at = at + self.interval
        time.sleep(max(0.0, at - now))

    def penalize(self, retry_after: float | None) -> None:
        with self.lock:
            self.interval = min(self.interval * 2, 8.0)
            pause = retry_after or max(5.0, self.interval * 4)
            self.next_at = max(self.next_at, time.monotonic() + pause)

    def reward(self) -> None:
        with self.lock:
            self.interval = max(self.base, self.interval * 0.97)


class Client:
    def __init__(self, interval: float):
        self.throttle = Throttle(interval)
        self.local = threading.local()

    @property
    def session(self) -> requests.Session:
        session = getattr(self.local, "session", None)
        if session is None:
            session = self.local.session = requests.Session()
            session.headers["User-Agent"] = USER_AGENT
        return session

    def request(self, method: str, url: str, *, attempts: int = 8, throttled: bool = True,
                **kwargs) -> requests.Response:
        error: object = None
        for attempt in range(attempts):
            if throttled:
                self.throttle.wait()
            try:
                response = self.session.request(method, url, timeout=60, **kwargs)
            except requests.RequestException as exc:
                error = exc
                time.sleep(min(60, 2 ** attempt))
                continue
            if response.status_code == 429 or response.status_code >= 500:
                error = f"HTTP {response.status_code}"
                retry_after = response.headers.get("Retry-After", "")
                if throttled:
                    self.throttle.penalize(float(retry_after) if retry_after.isdigit() and
                                           int(retry_after) > 0 else None)
                else:
                    time.sleep(min(60, 2 ** attempt))
                continue
            if throttled:
                self.throttle.reward()
            return response
        raise RuntimeError(f"giving up on {url}: {error}")

    def query(self, query: dict, max_items: int | None = None) -> list[dict]:
        """Run a Community API v2 JSON query, following cursors until exhausted."""
        collection, spec = next(iter(query.items()))
        items: list[dict] = []
        cursor = None
        while True:
            page = dict(spec, **({"cursor": cursor} if cursor else {}))
            response = self.request("POST", API, json={collection: page})
            if "json" not in response.headers.get("Content-Type", ""):
                raise RuntimeError(f"API returned {response.status_code} "
                                   f"{response.headers.get('Content-Type')} (blocked?)")
            payload = response.json()
            if payload.get("status") != "success":
                raise RuntimeError(f"API error: {payload.get('message') or payload}")
            items += payload["data"]["items"]
            cursor = payload["data"].get("next_cursor")
            if not cursor or (max_items and len(items) >= max_items):
                return items[:max_items] if max_items else items

    def download(self, url: str, dest_dir: Path, stem: str) -> Path | None:
        """Save url as dest_dir/stem.<ext> unless such a file exists; None if unavailable."""
        existing = sorted(dest_dir.glob(glob.escape(stem) + ".*"))
        existing = [p for p in existing if p.suffix != ".part"]
        if existing:
            return existing[0]
        response = self.request("GET", url, stream=True)
        content_type = response.headers.get("Content-Type", "").split(";")[0].strip().lower()
        if response.status_code != 200 or content_type.startswith("text/html"):
            response.close()
            return None
        suffix = EXTENSIONS.get(content_type) or Path(urlparse(url).path).suffix.lower() \
            or mimetypes.guess_extension(content_type) or ".bin"
        dest_dir.mkdir(parents=True, exist_ok=True)
        target = dest_dir / f"{stem}{suffix}"
        partial = target.with_name(target.name + ".part")
        with response, open(partial, "wb") as fh:
            for chunk in response.iter_content(1 << 16):
                fh.write(chunk)
        partial.replace(target)
        return target


class YouTubeTitles:
    """oEmbed title lookups, cached on disk so re-runs do not ask again."""

    def __init__(self, client: Client, cache_file: Path, enabled: bool):
        self.client, self.cache_file, self.enabled = client, cache_file, enabled
        self.lock = threading.Lock()
        try:
            self.cache: dict[str, str | None] = json.loads(cache_file.read_text())
        except (OSError, ValueError):
            self.cache = {}

    def get(self, url: str) -> str | None:
        if not self.enabled:
            return None
        with self.lock:
            if url in self.cache:
                return self.cache[url]
        title = None
        try:
            response = self.client.request("GET", "https://www.youtube.com/oembed",
                                           params={"url": url, "format": "json"},
                                           attempts=3, throttled=False)
            if response.ok:
                title = response.json().get("title")
        except (RuntimeError, ValueError):
            pass
        with self.lock:
            self.cache[url] = title
        return title

    def save(self) -> None:
        self.cache_file.parent.mkdir(parents=True, exist_ok=True)
        self.cache_file.write_text(json.dumps(self.cache, indent=1, sort_keys=True))


# ------------------------------------------------------------------------ articles


@dataclass
class Article:
    id: str
    title: str
    unit: str
    unit_slug: str
    board_id: str
    url: str
    author: str | None
    published: str | None
    updated: str | None
    views: int | None
    labels: list[str]
    tags: list[str]
    attachments: list[dict]
    body: str

    @property
    def dir(self) -> str:
        return f"{self.unit_slug}/{slugify(self.title)}-{self.id}"

    @property
    def path(self) -> str:
        return f"{self.dir}/article.md"

    @classmethod
    def from_api(cls, item: dict, board: dict) -> Article:
        def texts(key: str) -> list[str]:
            return [x["text"] for x in (item.get(key) or {}).get("items", []) if x.get("text")]

        revision = item.get("current_revision") or {}
        return cls(
            id=item["id"],
            title=" ".join((item.get("subject") or "Untitled").split()),
            unit=board["title"],
            unit_slug=slugify(board["title"]),
            board_id=board["id"],
            url=item.get("view_href") or KB_URL,
            author=(item.get("author") or {}).get("login"),
            published=item.get("post_time"),
            updated=revision.get("last_edit_time") or item.get("post_time"),
            views=(item.get("metrics") or {}).get("views"),
            labels=texts("labels"),
            tags=texts("tags"),
            attachments=(item.get("attachments") or {}).get("items", []),
            body=item.get("body") or "",
        )


def articles_query(board_id: str, page_size: int) -> dict:
    return {"messages": {
        "fields": ARTICLE_FIELDS,
        "constraints": [{"board.id": board_id}, {"depth": 0}],
        "sorts": ["post_time desc"],
        "limit": page_size,
        "subQueries": ARTICLE_SUBQUERIES,
    }}


# ---------------------------------------------------------------- HTML -> Markdown


class KBMarkdown(MarkdownConverter):
    def convert_figcaption(self, el, text, parent_tags):
        text = text.strip()
        return f"\n\n*{text}*\n\n" if text else ""


MARKDOWN_OPTIONS = dict(
    heading_style="ATX",
    bullets="-",
    escape_underscores=False,
    escape_misc=False,
    newline_style="backslash",
    table_infer_header=True,
    keep_inline_images_in=["td", "th", "a", "span", "p", "div", "li", "strong", "b", "em", "i",
                           "figure", "font", "u", *HEADINGS],
)


@dataclass
class Options:
    out: Path
    image_max_px: int
    videos: bool


class ArticleConverter:
    """Turns one article's Khoros HTML into Markdown, downloading media next to it."""

    def __init__(self, article: Article, known: dict[str, str], client: Client,
                 youtube: YouTubeTitles, options: Options):
        self.article, self.known, self.client = article, known, client
        self.youtube, self.options = youtube, options
        self.root = options.out / article.dir
        self.media_dir = self.root / "media"
        self.used_media: set[str] = set()
        self.videos: list[dict] = []
        self.images = 0
        self.missing_media: list[str] = []

    def convert(self) -> str:
        soup = BeautifulSoup(self.article.body, "html.parser")
        for tag in soup.find_all(["script", "style", "noscript", "button", "input"]):
            tag.decompose()
        for text in soup.find_all(string=lambda t: "\xa0" in t):
            # Markdown emphasis drops a non-breaking space at its edge: "**Tab**name"
            text.replace_with(text.replace("\xa0", " "))
        self.drop_table_of_contents(soup)
        self.flatten_details(soup)
        self.flatten_layout_tables(soup)
        self.rewrite_links(soup)  # before media, whose new links are already final
        self.rewrite_images(soup)
        self.rewrite_videos(soup)
        self.rewrite_iframes(soup)
        self.tidy_headings(soup)
        markdown = KBMarkdown(**MARKDOWN_OPTIONS).convert_soup(soup)
        return clean_markdown(markdown)

    # --- structure

    def is_self_link(self, href: str) -> bool:
        return href.startswith("#") or (f"/ta-p/{self.article.id}" in href and "#" in href)

    def drop_table_of_contents(self, soup: BeautifulSoup) -> None:
        for box in soup.find_all(["table", "ul", "ol", "div"]):
            if box.decomposed:
                continue
            links = box.find_all("a", href=True)
            if len(links) < 2 or not all(self.is_self_link(a["href"]) for a in links):
                continue
            link_text = sum(len(a.get_text(strip=True)) for a in links)
            if link_text < 0.6 * len(box.get_text(strip=True)):
                continue
            heading = box.find_previous_sibling()
            if heading and re.fullmatch(r"(table of )?contents:?", heading.get_text(" ", True), re.I):
                heading.decompose()
            box.decompose()

    @staticmethod
    def flatten_details(soup: BeautifulSoup) -> None:
        for summary in soup.find_all("summary"):
            if summary.find(HEADINGS + ["p", "div"]):
                summary.unwrap()
                continue
            for bold in summary.find_all(["strong", "b"]):
                bold.unwrap()
            strong = soup.new_tag("strong")
            for child in list(summary.contents):
                strong.append(child.extract())
            summary.append(strong)
            summary.name = "p"
        for details in soup.find_all("details"):
            details.unwrap()

    @staticmethod
    def flatten_layout_tables(soup: BeautifulSoup) -> None:
        # Innermost first, so a layout table wrapping a data table keeps the data table.
        for table in reversed(soup.find_all("table")):
            rows = [r for r in table.find_all("tr") if r.find_parent("table") is table]
            columns = max((len(r.find_all(["td", "th"], recursive=False)) for r in rows), default=0)
            if columns > 1 and not table.find("table"):
                continue
            box = soup.new_tag("div")
            for row in rows:
                for cell in row.find_all(["td", "th"], recursive=False):
                    part = soup.new_tag("div")
                    for child in list(cell.contents):
                        part.append(child.extract())
                    box.append(part)
            table.replace_with(box)

    @staticmethod
    def tidy_headings(soup: BeautifulSoup) -> None:
        headings = soup.find_all(HEADINGS)
        shift = 1 if any(h.name == "h1" for h in headings) else 0  # "# title" stays unique
        for h in headings:
            if not h.get_text(strip=True):
                if h.find("img"):
                    h.name = "p"
                else:
                    h.decompose()
                continue
            for emphasis in h.find_all(["strong", "b", "em", "i", "u", "font"]):
                emphasis.unwrap()
            h.name = f"h{min(6, int(h.name[1]) + shift)}"

    # --- media

    def rewrite_images(self, soup: BeautifulSoup) -> None:
        for img in soup.find_all("img"):
            src = (img.get("src") or "").strip()
            if not src or src.startswith(("data:", "./", "file:")):
                img.decompose()
                continue
            src = urljoin(BASE + "/", src)
            if LEGACY_ICON.search(urlparse(src).path) or is_icon_sized(img):
                img.decompose()
                continue
            caption_el, wrapper = self.find_caption(img)
            alt = describe_image(img, caption_el.get_text(" ", True) if caption_el else "")
            local = self.fetch_image(src)
            new = soup.new_tag("img", src=local or requests.utils.requote_uri(src),
                               alt=alt.replace("[", "(").replace("]", ")"))
            img.replace_with(new)
            self.images += 1
            if caption_el is not None and len(wrapper.find_all("img")) == 1:
                caption_el.decompose()
            anchor = new.find_parent("a")
            if anchor is not None and not anchor.get_text(strip=True):
                href = anchor.get("href") or ""
                if not href or KHOROS_IMAGE.search(href) or "/legacyfs/" in href or \
                        re.search(r"\.(png|jpe?g|gif|webp|bmp)$", urlparse(href).path, re.I):
                    anchor.unwrap()  # lightbox link to the image itself

    @staticmethod
    def find_caption(img: Tag) -> tuple[Tag | None, Tag | None]:
        for wrapper in img.parents:
            if wrapper.name == "figure" or "lia-inline-image-display-wrapper" in (wrapper.get("class") or []):
                caption = wrapper.find("figcaption") or wrapper.find(class_="lia-inline-image-caption")
                return caption, wrapper
            if wrapper.name in ("p", "div", "td", "li", "body", "[document]"):
                break
        return None, None

    def fetch_image(self, src: str) -> str | None:
        match = KHOROS_IMAGE.search(src)
        if match:
            size = (f"image-dimensions/{self.options.image_max_px}x{self.options.image_max_px}?v=v2"
                    if self.options.image_max_px else "image-size/original?v=v2&px=-1")
            url = f"{BASE}/t5/image/serverpage/image-id/{match.group(1)}/{size}"
            stem = f"img-{match.group(2)}"
        else:
            url = requests.utils.requote_uri(src)
            stem = f"img-{slugify(Path(unquote(urlparse(src).path)).stem, 40)}-" \
                   f"{hashlib.sha1(src.encode()).hexdigest()[:6]}"
        return self.fetch(url, stem)

    def fetch(self, url: str, stem: str) -> str | None:
        try:
            path = self.client.download(url, self.media_dir, stem)
        except RuntimeError as exc:
            log(f"    ! {self.article.id}: {exc}")
            path = None
        if path is None:
            self.missing_media.append(url)
            return None
        self.used_media.add(path.name)
        return f"media/{path.name}"

    def rewrite_videos(self, soup: BeautifulSoup) -> None:
        for video in soup.find_all("video"):
            source = video.get("src") or (video.find("source") or {}).get("src")
            if not source:
                video.decompose()
                continue
            source = urljoin(BASE + "/", source.strip())
            name = Path(unquote(urlparse(source).path)).stem
            local = self.fetch(requests.utils.requote_uri(source), f"vid-{slugify(name)}") \
                if self.options.videos else None
            self.videos.append({"kind": "video", "url": source, "title": name, "local": local})
            video.replace_with(self.media_paragraph(soup, "Video", name, local or source))

    def rewrite_iframes(self, soup: BeautifulSoup) -> None:
        for frame in soup.find_all("iframe"):
            src = (frame.get("src") or frame.get("data-src") or "").strip()
            kind, url = classify_embed(urljoin(BASE + "/", src)) if src else ("", None)
            if not url:
                frame.decompose()
                continue
            title = None
            if kind == "YouTube video":
                title = self.youtube.get(url)
            title = title or frame.get("title") or url
            self.videos.append({"kind": kind, "url": url, "title": title, "local": None})
            frame.replace_with(self.media_paragraph(soup, kind, title, url))

    @staticmethod
    def media_paragraph(soup: BeautifulSoup, label: str, title: str, href: str) -> Tag:
        p = soup.new_tag("p")
        strong = soup.new_tag("strong")
        strong.string = f"{label}:"
        link = soup.new_tag("a", href=href)
        link.string = title
        p.extend([strong, " ", link])
        return p

    # --- links

    def rewrite_links(self, soup: BeautifulSoup) -> None:
        here = self.article.dir
        for a in soup.find_all("a"):
            href = (a.get("href") or "").strip()
            if not href or self.is_self_link(href) or href.lower().startswith("javascript:"):
                a.unwrap()
                continue
            absolute = urljoin(BASE + "/", href)
            parsed = urlparse(absolute)
            match = TOPIC_LINK.search(parsed.path)
            if match and parsed.netloc.lower().endswith("community.graphisoft.com"):
                topic = match.group(1)
                if topic == self.article.id:
                    a.unwrap()
                    continue
                if topic in self.known:
                    a.attrs = {"href": posixpath.relpath(self.known[topic], here)}
                    continue
            a.attrs = {"href": requests.utils.requote_uri(absolute)}

    # --- attachments

    def save_attachments(self) -> list[dict]:
        saved = []
        folder = self.root / "attachments"
        keep = set()
        for item in self.article.attachments:
            filename = re.sub(r"[/\\:]", "_", item.get("filename") or "attachment").strip() or "attachment"
            stem, suffix = Path(filename).stem, Path(filename).suffix
            path = None
            if item.get("url"):
                try:
                    path = self.client.download(item["url"], folder, stem)
                except RuntimeError as exc:
                    log(f"    ! {self.article.id}: {exc}")
            if path and suffix and path.suffix != suffix:  # keep the uploaded extension
                path = path.replace(path.with_suffix(suffix))
            if path:
                keep.add(path.name)
            saved.append({"filename": filename, "size": item.get("filesize"), "url": item.get("url"),
                          "local": f"attachments/{path.name}" if path else None})
        remove_unused(folder, keep)
        return saved


def is_icon_sized(img: Tag) -> bool:
    sizes = [img.get("width", ""), img.get("height", "")]
    return all(str(v).isdigit() and int(v) <= 32 for v in sizes)


def describe_image(img: Tag, caption: str) -> str:
    for candidate in (caption, img.get("alt"), img.get("title")):
        candidate = " ".join((candidate or "").split())
        if candidate and not FILENAME_LIKE.match(candidate):
            return candidate
    return ""


def classify_embed(src: str) -> tuple[str, str | None]:
    parsed = urlparse(src)
    host = parsed.netloc.lower()
    query = parse_qs(parsed.query)
    if "embedly.com" in host:
        inner = (query.get("url") or query.get("src") or [None])[0]
        return classify_embed(inner) if inner else ("Embedded content", None)
    if "youtube" in host or host.endswith("youtu.be"):
        if "list" in query and "videoseries" in parsed.path:
            return "YouTube playlist", f"https://www.youtube.com/playlist?list={query['list'][0]}"
        if "v" in query:
            return "YouTube video", f"https://www.youtube.com/watch?v={query['v'][0]}"
        video_id = parsed.path.rstrip("/").split("/")[-1]
        video_id = (re.match(r"[\w-]{11}", video_id) or re.match(r".*", video_id)).group(0)  # drop pasted junk
        return ("YouTube video", f"https://www.youtube.com/watch?v={video_id}") if video_id \
            else ("Embedded content", src)
    if "vimeo.com" in host:
        return "Vimeo video", f"https://vimeo.com/{parsed.path.rstrip('/').split('/')[-1]}"
    if "bimx" in host:
        return "BIMx model", src
    return ("Embedded content", src) if parsed.scheme in ("http", "https") else ("Embedded content", None)


def clean_markdown(text: str) -> str:
    text = text.replace("\xa0", " ").replace("\u200b", "").replace("\ufeff", "")
    text = re.sub(r"[ \t]+$", "", text, flags=re.M)
    text = re.sub(r"^[ \t]*\\$", "", text, flags=re.M)  # hard breaks left alone on a line
    text = re.sub(r"[ \t]*\\$(?=\n[ \t]*(?:\n|$)|\Z)", "", text, flags=re.M)  # ...or ending a paragraph
    text = re.sub(r"^\*\*\s*\*\*$", "", text, flags=re.M)  # empty bold paragraphs
    text = re.sub(r"(?<=\w)\*\*([\u2010\u2011-])\*\*(?=\w)", r"\1", text)  # stand**‑**alone
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip() + "\n"


def summarize(markdown: str, limit: int = 220) -> str:
    for block in markdown.split("\n\n"):
        block = block.strip()
        if not block or block.startswith(("#", "!", "|", ">", "---", "**Video", "**YouTube",
                                          "**BIMx", "**Embedded", "**Vimeo")):
            continue
        text = re.sub(r"!\[[^\]]*\]\([^)]*\)", "", block)
        text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
        text = " ".join(re.sub(r"[*`\\]|^\s*(?:-|\d+\.)\s", "", text, flags=re.M).split())
        if len(text) < 40:
            continue
        return text if len(text) <= limit else text[:limit].rsplit(" ", 1)[0] + "…"
    return ""


def remove_unused(folder: Path, keep: set[str]) -> None:
    if folder.is_dir():
        for path in folder.iterdir():
            if path.is_file() and path.name not in keep:
                path.unlink()


def front_matter(fields: dict) -> str:
    lines = [f"{key}: {json.dumps(value, ensure_ascii=False)}" for key, value in fields.items()]
    return "---\n" + "\n".join(lines) + "\n---\n"


def day(timestamp: str | None) -> str | None:
    return timestamp[:10] if timestamp else None


def process(article: Article, known: dict[str, str], client: Client, youtube: YouTubeTitles,
            options: Options) -> dict:
    converter = ArticleConverter(article, known, client, youtube, options)
    body = converter.convert()
    attachments = converter.save_attachments()
    remove_unused(converter.media_dir, converter.used_media)

    meta = {
        "id": article.id,
        "title": article.title,
        "unit": article.unit,
        "board_id": article.board_id,
        "url": article.url,
        "author": article.author,
        "published": day(article.published),
        "updated": day(article.updated),
        "views": article.views,
        "labels": article.labels,
        "tags": article.tags,
        "images": converter.images,
        "videos": [v["local"] or v["url"] for v in converter.videos],
        "attachments": [a["local"] or a["url"] for a in attachments],
    }
    parts = [
        front_matter(meta),
        f"# {article.title}\n",
        f"> Archicad Knowledge Base · {article.unit} · updated {day(article.updated)} · "
        f"source: <{article.url}>\n",
        body,
    ]
    if attachments:
        lines = []
        for a in attachments:
            size = f" ({a['size'] / 1024:.0f} KB)" if a.get("size") else ""
            lines.append(f"- [{a['filename']}](<{a['local'] or a['url']}>){size}")
        parts.append("## Attachments\n\n" + "\n".join(lines) + "\n")
    converter.root.mkdir(parents=True, exist_ok=True)
    target = converter.root / "article.md"
    partial = target.with_name("article.md.part")
    partial.write_text("\n".join(parts), encoding="utf-8")
    partial.replace(target)

    return {
        **meta,
        "unit_slug": article.unit_slug,
        "path": article.path,
        "summary": summarize(body),
        "word_count": len(re.findall(r"\w+", re.sub(r"\(media/[^)]*\)", "", body))),
        "videos": converter.videos,
        "attachments": attachments,
        "missing_media": converter.missing_media,
    }


# ------------------------------------------------------------------------- outputs


def write_index(out: Path, entries: list[dict], unit_order: list[str], scraped_at: str) -> None:
    units = sorted({e["unit"] for e in entries},
                   key=lambda u: (unit_order.index(u) if u in unit_order else len(unit_order), u))
    lines = [
        "# Archicad Knowledge Base — article index",
        "",
        f"{len(entries)} articles from <{KB_URL}>, scraped {scraped_at[:10]}. "
        "Paths are relative to this file; see README.md for the layout.",
        "",
    ]
    for unit in units:
        group = sorted((e for e in entries if e["unit"] == unit), key=lambda e: e["title"].lower())
        lines += [f"## {unit} ({len(group)})", ""]
        for e in group:
            labels = f" `{', '.join(e['labels'])}`" if e["labels"] else ""
            summary = f" — {e['summary']}" if e["summary"] else ""
            lines.append(f"- [{e['title']}]({e['path']}){summary}{labels}")
        lines.append("")
    (out / "index.md").write_text("\n".join(lines), encoding="utf-8")


def write_readme(out: Path, entries: list[dict], scraped_at: str, image_max_px: int) -> None:
    counts: dict[str, int] = {}
    for e in entries:
        counts[e["unit"]] = counts.get(e["unit"], 0) + 1
    units = "\n".join(f"- `{next(x['unit_slug'] for x in entries if x['unit'] == u)}/` — "
                      f"{u} ({n} articles)" for u, n in counts.items())
    images = sum(e["images"] for e in entries)
    image_size = (f"Images were downloaded with their long edge capped at {image_max_px} px."
                  if image_max_px else "Images are the original uploads.")
    readme = f"""# Archicad Knowledge Base (offline corpus)

Articles from the official Graphisoft Archicad Knowledge Base
(<{KB_URL}>), converted to Markdown for coding agents.
Scraped {scraped_at[:10]}: {len(entries)} articles, {images} images.

## Layout

{units}

Each article lives in `<unit>/<slug>-<id>/`:

- `article.md` — YAML front matter (id, title, unit, source url, author, published/updated
  dates, views, labels, tags, media counts) followed by the article as Markdown. Headings
  start at `#` for the title.
- `media/` — images (`img-*`) and Graphisoft-hosted video clips (`vid-*.mp4`) referenced by
  relative links in `article.md`. Image alt text holds the original caption when there was one.
- `attachments/` — files attached to the article (rare; e.g. `.gsm` library parts).

YouTube, BIMx and other embeds are kept as links, written as `**YouTube video:** [title](url)`.
Links to other scraped articles are relative paths; everything else points to the web.

## Finding things

- `index.md` lists every article per unit with a one-line summary and its labels.
- `manifest.json` has the same data plus paths, word counts, video and attachment details.
- Full-text search works well, e.g. `grep -ril "curtain wall" --include=article.md .`
- Labels (`labels:` in front matter) are the KB's own topic taxonomy.

## Caveats

- Articles cover many Archicad versions; check `updated` and the text for the version
  an instruction applies to.
- {image_size}
- Content belongs to Graphisoft and its authors; cite the `url` from the front matter.
"""
    (out / "README.md").write_text(readme, encoding="utf-8")


# ---------------------------------------------------------------------------- main


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0],
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--units", nargs="+", metavar="UNIT",
                        help="unit titles or board ids (default: Getting started, Modeling, "
                             "Documentation, Visualization)")
    parser.add_argument("--out", default=Path(__file__).resolve().parent / "archicad-kb",
                        help="output directory (default: archicad-kb/ next to this script)")
    parser.add_argument("--limit", type=int, help="only the N newest articles per unit (for testing)")
    parser.add_argument("--image-max-px", type=int, default=1600,
                        help="cap image long edge in px; 0 downloads originals (default: %(default)s)")
    parser.add_argument("--no-videos", action="store_true",
                        help="link Graphisoft-hosted .mp4 clips instead of downloading them")
    parser.add_argument("--no-youtube-titles", action="store_true",
                        help="do not look up YouTube titles via oEmbed")
    parser.add_argument("--workers", type=int, default=4, help="articles processed in parallel")
    parser.add_argument("--interval", type=float, default=0.4,
                        help="minimum seconds between requests to the community site; "
                             "grows automatically when rate-limited (default: %(default)s)")
    parser.add_argument("--list-units", action="store_true", help="list the KB units and exit")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    client = Client(args.interval)
    boards = client.query({"boards": {"fields": ["id", "title"],
                                      "constraints": [{"parent_category.id": KB_CATEGORY}],
                                      "limit": 100}})
    if args.list_units:
        for board in boards:
            print(f"{board['id']:28} {board['title']}")
        return 0

    lookup = {}
    for board in boards:
        lookup[normalize_unit(board["id"])] = lookup[normalize_unit(board["title"])] = board
    selected = []
    for name in args.units or DEFAULT_UNITS:
        board = lookup.get(normalize_unit(name))
        if not board:
            sys.exit(f"unknown unit {name!r}; available: " + ", ".join(b["title"] for b in boards))
        if board not in selected:
            selected.append(board)

    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    options = Options(out=out, image_max_px=args.image_max_px, videos=not args.no_videos)
    youtube = YouTubeTitles(client, out / ".cache" / "youtube-titles.json", not args.no_youtube_titles)

    articles: list[Article] = []
    for board in selected:
        items = client.query(articles_query(board["id"], min(args.limit or 50, 50)), args.limit)
        log(f"{board['title']}: {len(items)} articles")
        articles += [Article.from_api(item, board) for item in items]

    # Keep entries of units not scraped this time, and let links point at them.
    try:
        previous = json.loads((out / "manifest.json").read_text())["articles"]
    except (OSError, ValueError, KeyError):
        previous = []
    selected_ids = {b["id"] for b in selected}
    kept = [e for e in previous if e["board_id"] not in selected_ids and (out / e["path"]).exists()]
    known = {e["id"]: e["path"] for e in kept} | {a.id: a.path for a in articles}

    entries, failures = [], []
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        futures = {pool.submit(process, a, known, client, youtube, options): a for a in articles}
        for done, future in enumerate(as_completed(futures), 1):
            article = futures[future]
            try:
                entry = future.result()
            except Exception as exc:  # keep going; report at the end
                failures.append((article, exc))
                log(f"[{done}/{len(articles)}] FAILED {article.unit}: {article.title}: {exc!r}")
                continue
            entries.append(entry)
            missing = f", {len(entry['missing_media'])} media unavailable" if entry["missing_media"] else ""
            log(f"[{done}/{len(articles)}] {article.unit}: {article.title} "
                f"({entry['images']} images, {len(entry['videos'])} videos{missing})")
    youtube.save()

    scraped_at = datetime.now(timezone.utc).isoformat(timespec="seconds")
    failed_ids = {a.id for a, _ in failures}
    # A failed article keeps its previous entry if it had one.
    entries += [e for e in previous if e["id"] in failed_ids and (out / e["path"]).exists()]
    all_entries = sorted(kept + entries, key=lambda e: (e["unit"], e["title"].lower()))
    manifest = {"source": KB_URL, "scraped_at": scraped_at, "articles": all_entries}
    (out / "manifest.json").write_text(json.dumps(manifest, indent=1, ensure_ascii=False),
                                       encoding="utf-8")
    unit_order = [b["title"] for b in selected] + [b["title"] for b in boards]
    write_index(out, all_entries, unit_order, scraped_at)
    write_readme(out, all_entries, scraped_at, args.image_max_px)

    if not args.limit:
        current = {posixpath.dirname(e["path"]) for e in all_entries}
        for board in selected:
            unit_dir = out / slugify(board["title"])
            if not unit_dir.is_dir():
                continue
            for stale in sorted(p for p in unit_dir.iterdir() if p.is_dir()):
                if f"{unit_dir.name}/{stale.name}" not in current:
                    log(f"note: {stale.relative_to(out)} is no longer in the KB (left in place)")

    log(f"\nWrote {len(entries)} articles to {out}" +
        (f"; {len(failures)} failed: " + ", ".join(a.id for a, _ in failures) if failures else ""))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
