# Lists the simple commands a shell command line runs, for the loop's Bash guard (guard-bash.sh).
# Input: the command line. Output: one command per line, its words separated by tabs, quotes
# removed. The first word is the program that runs: keywords (then, do, ...), variable assignments
# and the wrappers in WRAP (sudo, env, xargs, timeout, ...) are dropped. A write redirection is
# listed as ">" and its target (/dev/null, /dev/stdout and /dev/stderr left out). The commands
# inside $(...), `...`, <(...), >(...) and find -exec are listed too, and the scripts of sh -c,
# eval, watch, ssh, parallel and env -S, and what a shell reads on its standard input (a heredoc,
# or every argument on a line that pipes into it) are scanned as command lines; the text of a
# substitution also stays in the word it belongs to. Other heredoc bodies and quoted words are
# data. It reads the common cases, it is not a shell: a guardrail, not a sandbox. POSIX awk (BSD
# awk on macOS, mawk on Ubuntu).

BEGIN {
    TAB = "\t"
    NL = "\036" # stands for a newline inside a word
    # Wrappers that run the rest of their words as a command: the options that take their value
    # as the next word, and OPERANDS, the words between the options and the command.
    WRAP["sudo"] = "^-[ugCDhpRTU]$|^--(user|group|host|prompt|chdir|close-from|role|type|other-user)$"
    WRAP["doas"] = "^-[uC]$"
    WRAP["env"] = "^-[uCPS]$|^--(unset|chdir|split-string)$"
    WRAP["nice"] = "^-n$|^--adjustment$"
    WRAP["ionice"] = "^-[cnpPu]$|^--(class|classdata|pid|pgid|uid)$"
    WRAP["timeout"] = WRAP["gtimeout"] = "^-[sk]$|^--(signal|kill-after)$"
    OPERANDS["timeout"] = OPERANDS["gtimeout"] = 1 # the duration
    WRAP["stdbuf"] = "^-[ioe]$|^--(input|output|error)$"
    WRAP["caffeinate"] = "^-[tw]$"
    WRAP["xargs"] = "^-[EIJLRSPadns]$|^--(arg-file|delimiter|max-args|max-procs|max-chars|process-slot-var)$"
    WRAP["exec"] = "^-a$"
    WRAP["command"] = WRAP["builtin"] = WRAP["time"] = WRAP["nohup"] = WRAP["setsid"] = "^$"
    WRAP["chronic"] = WRAP["unbuffer"] = WRAP["valgrind"] = "^$"
    WRAP["strace"] = "^-[abeEIoOpPsSuX]$"; WRAP["ltrace"] = "^-[aADeFlnopsuwx]$"
    # GNU parallel's options that take their value as the next word; its command (the words up to
    # ::: or ::::) runs through a shell.
    PARALLEL_OPTS = "^-[jSadICNnLEsP]$|^--(jobs|sshlogin|arg-file|delimiter|colsep|max-args|max-lines|max-chars|workdir|wd|tmpdir|timeout|joblog|results|tagstring|tag-string|env|halt|delay|retries|memfree|load|nice|basefile|bf|return|transferfile|tf)$"
}
{ text = text (NR > 1 ? "\n" : "") $0 }
END {
    nq = 1
    queue[1] = text
    for (q = 1; q <= nq; q++) scan(queue[q])
}

# Queues a script (sh -c, eval, watch, ssh, env -S, a shell's standard input) to be scanned as a
# command line.
function enqueue(script) {
    if (nq >= 20) return
    gsub(NL, "\n", script)
    queue[++nq] = script
}

# Each nesting level (the line itself, then every open substitution) has its own word, words,
# quoting state, pending redirection and pending heredocs.
function open_level(l, closer, start) {
    word[l] = ""; inword[l] = 0; nw[l] = 0; state[l] = ""; redir[l] = ""; depth[l] = 0; nhd[l] = 0
    close_char[l] = closer; begin[l] = start
}

function add(str) {
    gsub(/\n/, NL, str)
    gsub(/\t/, " ", str)
    word[L] = word[L] str
    inword[L] = 1
}

function end_word() {
    if (!inword[L]) return
    if (redir[L] == ">") {
        if (word[L] !~ /^\/dev\/(null|stdout|stderr)$/) print ">" TAB word[L]
    } else if (redir[L] != "<") wd[L, ++nw[L]] = word[L]
    redir[L] = ""; word[L] = ""; inword[L] = 0
}

function end_command(    k, a) {
    end_word()
    redir[L] = ""
    for (k = 1; k <= nw[L]; k++) a[k] = wd[L, k]
    if (nw[L]) run(a, 1, nw[L])
    nw[L] = 0
}

function scan(s,    n, i, c, c2) {
    n = length(s)
    L = 0
    shell_stdin = 0
    piped = ""
    open_level(0, "", 1)
    i = 1
    while (i <= n) {
        c = substr(s, i, 1)
        c2 = substr(s, i, 2)
        if (state[L] == "'") { # '...': literal
            if (c == "'") state[L] = ""; else add(c)
            i++; continue
        }
        if (state[L] == "$'") { # $'...': backslash escapes
            if (c == "\\") { add(substr(s, i + 1, 1)); i += 2; continue }
            if (c == "'") state[L] = ""; else add(c)
            i++; continue
        }
        if (state[L] == "\"") { # "...": substitutions stay live
            if (c == "\\" && i < n && index("\"\\$`\n", substr(s, i + 1, 1))) {
                if (substr(s, i + 1, 1) != "\n") add(substr(s, i + 1, 1))
                i += 2; continue
            }
            if (c == "\"") { state[L] = ""; i++; continue }
            if (c2 == "$(" || c == "`") { i = substitution(s, i); continue }
            add(c); i++; continue
        }
        if (c == "\\") { # an escaped character, or a line continuation
            if (substr(s, i + 1, 1) != "\n") add(substr(s, i + 1, 1))
            i += 2; continue
        }
        if (c == "'") { state[L] = "'"; inword[L] = 1; i++; continue }
        if (c2 == "$'") { state[L] = "$'"; inword[L] = 1; i += 2; continue }
        if (c == "\"") { state[L] = "\""; inword[L] = 1; i++; continue }
        if (c2 == "$(" || c2 == "<(" || c2 == ">(") { i = substitution(s, i); continue }
        if (c == "`") { i = close_char[L] == "`" ? close_level(s, i) : substitution(s, i); continue }
        if (c == "#" && !inword[L]) { # a comment, up to the end of the line
            while (i <= n && substr(s, i, 1) != "\n") i++
            continue
        }
        if (c == " " || c == "\t") { end_word(); i++; continue }
        if (c == "\n") {
            end_command()
            i = nhd[L] ? heredocs(s, i + 1) : i + 1
            continue
        }
        if (c == ";" || c == "|" || (c == "&" && c2 != "&>")) { end_command(); i++; continue }
        if (c == "(") { end_command(); depth[L]++; i++; continue }
        if (c == ")") {
            if (depth[L] > 0) { end_command(); depth[L]--; i++; continue }
            if (close_char[L] == ")") { i = close_level(s, i); continue }
            end_command(); i++; continue
        }
        if (c == ">" || c2 == "&>") { i = redirect_out(s, i); continue }
        if (c == "<") { i = redirect_in(s, i); continue }
        add(c); i++
    }
    while (L > 0) { end_command(); L-- } # an unclosed substitution
    end_command()
    # A shell that reads its script from standard input (echo rm x | sh): every argument on the
    # line may be part of that script, so each is scanned as a command line too.
    if (shell_stdin) enqueue(piped)
}

# $((...)) stays text; $(...), `...`, <(...) and >(...) open a level whose commands are listed.
function substitution(s, i,    j, d, c) {
    if (substr(s, i, 3) == "$((") {
        d = 0
        for (j = i + 1; j <= length(s); j++) {
            c = substr(s, j, 1)
            if (c == "(") d++
            else if (c == ")" && --d == 0) break
        }
        add(substr(s, i, j - i + 1))
        return j + 1
    }
    L++
    if (substr(s, i, 1) == "`") { open_level(L, "`", i); return i + 1 }
    open_level(L, ")", i)
    return i + 2
}

# Ends a substitution; its whole text becomes part of the enclosing word.
function close_level(s, i,    raw) {
    end_command()
    raw = substr(s, begin[L], i - begin[L] + 1)
    L--
    add(raw)
    return i + 1
}

# >, >>, >|, &>, &>>, N>: the next word is a write target. >&N and >&- duplicate a descriptor.
function redirect_out(s, i,    c) {
    if (inword[L] && word[L] ~ /^[0-9]+$/) { word[L] = ""; inword[L] = 0 } else end_word()
    if (substr(s, i, 1) == "&") i++
    i++
    c = substr(s, i, 1)
    if (c == "&") {
        i++
        if (substr(s, i, 1) ~ /[0-9-]/) {
            while (substr(s, i, 1) ~ /[0-9-]/) i++
            return i
        }
    } else if (c == ">" || c == "|") i++
    redir[L] = ">"
    return i
}

# <, <<<, <&N, <>, and heredocs (<<WORD, <<-WORD, quoted WORD): the body is read at the next newline.
function redirect_in(s, i,    j, c, delim, tabs) {
    if (inword[L] && word[L] ~ /^[0-9]+$/) { word[L] = ""; inword[L] = 0 } else end_word()
    if (substr(s, i, 3) == "<<<") { redir[L] = "<"; return i + 3 }
    if (substr(s, i, 2) == "<&") {
        i += 2
        while (substr(s, i, 1) ~ /[0-9-]/) i++
        return i
    }
    if (substr(s, i, 2) == "<>") { redir[L] = ">"; return i + 2 }
    if (substr(s, i, 2) != "<<") { redir[L] = "<"; return i + 1 }
    i += 2
    if (substr(s, i, 1) == "-") { tabs = 1; i++ }
    while (substr(s, i, 1) == " " || substr(s, i, 1) == "\t") i++
    delim = ""
    while (i <= length(s)) {
        c = substr(s, i, 1)
        if (index(" \t\n;&|<>()", c)) break
        if (c == "'" || c == "\"") {
            j = index(substr(s, i + 1), c)
            if (!j) break
            delim = delim substr(s, i + 1, j - 1)
            i += j + 1
            continue
        }
        if (c == "\\") { delim = delim substr(s, i + 1, 1); i += 2; continue }
        delim = delim c
        i++
    }
    if (delim != "") { nhd[L]++; hd[L, nhd[L]] = delim; hdtabs[L, nhd[L]] = tabs }
    return i
}

# Reads the pending heredoc bodies from position p on and returns the position after the last
# delimiter line. A body is data unless the line runs a shell that reads its standard input. A
# heredoc without its delimiter line is not consumed, so the rest is scanned as commands.
function heredocs(s, p,    n, q, k, e, line, body) {
    n = length(s)
    q = p
    for (k = 1; k <= nhd[L]; k++) {
        body = ""
        while (1) {
            if (q > n) { nhd[L] = 0; return p }
            e = index(substr(s, q), "\n")
            line = e ? substr(s, q, e - 1) : substr(s, q)
            q = e ? q + e : n + 1
            if (hdtabs[L, k]) sub(/^\t+/, "", line)
            if (line == hd[L, k]) break
            body = body line "\n"
        }
        if (shell_stdin) enqueue(body)
    }
    nhd[L] = 0
    return q
}

# Prints the command in words a[k..m] without its keywords, assignments and wrappers, then the
# commands it runs in turn: find -exec, the scripts of sh -c, eval, watch, ssh, parallel and
# env -S. A shell without a script file reads its standard input (shell_stdin).
function run(a, k, m,    w, j, prog, line, b, nb, script, opt_c, opt_s) {
    while (k <= m) {
        if (a[k] ~ /^[A-Za-z_][A-Za-z0-9_]*\+?=/) { k++; continue }
        w = a[k]
        sub(/.*\//, "", w)
        if (w ~ /^(!|\{|\}|then|do|else|elif|if|while|until|coproc)$/) { k++; continue }
        if (w == "function") { k += 2; continue } # function NAME { ...
        if (w == "command" && a[k + 1] ~ /^-[vV]$/) break # command -v: a lookup
        if (w in WRAP) {
            for (k++; k <= m && a[k] ~ /^-/; k++) {
                if (w == "env" && a[k] ~ /^(-S|--split-string)$/) enqueue(a[k + 1])
                if (a[k] ~ WRAP[w]) k++
            }
            k += OPERANDS[w]
            continue
        }
        break
    }
    if (k > m) return
    prog = a[k]
    sub(/.*\//, "", prog)
    line = prog
    for (j = k + 1; j <= m; j++) {
        line = line TAB a[j]
        piped = piped "\n" a[j]
    }
    print line
    if (prog == "find") {
        for (j = k + 1; j <= m; j++) {
            if (a[j] !~ /^-(exec|execdir|ok|okdir)$/) continue
            nb = 0
            for (j++; j <= m && a[j] != ";" && a[j] != "+"; j++) b[++nb] = a[j]
            if (nb) run(b, 1, nb)
        }
    } else if (prog ~ /^(ba|z|da|k)?sh$/) {
        for (j = k + 1; j <= m && a[j] ~ /^[-+]/; j++) {
            if (a[j] ~ /^-[A-Za-z]*c[A-Za-z]*$/) opt_c = 1
            if (a[j] ~ /^-[A-Za-z]*s[A-Za-z]*$/) opt_s = 1
            if (a[j] ~ /^[-+]o$/) j++
        }
        if (opt_c && j <= m) enqueue(a[j])
        else if (!opt_c && (opt_s || j > m || a[j] == "-")) shell_stdin = 1 # no script file
    } else if (prog ~ /^(eval|watch|ssh|parallel)$/) { # these run their words through a shell
        j = k + 1
        if (prog == "watch") for (; j <= m && a[j] ~ /^-/; j++) if (a[j] ~ /^(-n|--interval)$/) j++
        if (prog == "ssh") {
            for (; j <= m && a[j] ~ /^-/; j++) if (a[j] ~ /^-[BbcDEeFIiJLlmOoPpQRSWw]$/) j++
            j++ # the host
        }
        if (prog == "parallel") for (; j <= m && a[j] ~ /^-/; j++) if (a[j] ~ PARALLEL_OPTS) j++
        script = ""
        for (; j <= m && !(prog == "parallel" && a[j] ~ /^::::?\+?$/); j++) script = script " " a[j]
        enqueue(script)
    }
}
