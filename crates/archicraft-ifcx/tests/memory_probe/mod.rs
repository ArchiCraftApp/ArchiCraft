//! Measuring the memory of the IFCX reader and writer (NFR-SEC-003,
//! archicraft-4rrc): a global allocator that counts the heap and its peak,
//! and the input of the fuzz finding, many valid slabs at the point limit.
//! Shared by `tests/memory.rs` and `examples/memory.rs`; docs/testing.md
//! (Fuzzing) records the measurements.

// A global allocator implements the unsafe trait `GlobalAlloc`. This one only
// forwards every call unchanged to the system allocator and counts bytes.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use uuid::Uuid;

/// The system allocator, counting the bytes in use and their peak. Install it
/// with `#[global_allocator] static HEAP: CountingAllocator = CountingAllocator;`.
#[derive(Debug)]
pub struct CountingAllocator;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grow(bytes: usize) {
    let now = CURRENT.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

fn shrink(bytes: usize) {
    CURRENT.fetch_sub(bytes, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract for `layout` is passed on unchanged.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as for `alloc`.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by `System` with `layout`, as the caller
        // guarantees for this allocator.
        unsafe { System.dealloc(ptr, layout) };
        shrink(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: as for `dealloc`; `new_size` is the caller's.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            // A moving reallocation holds both blocks for a moment: count the
            // new block before releasing the old one, so the peak includes it.
            grow(new_size);
            shrink(layout.size());
        }
        new
    }
}

/// Starts a measurement: the peak is set to the bytes in use now, which are
/// returned.
pub fn start() -> usize {
    let now = CURRENT.load(Ordering::Relaxed);
    PEAK.store(now, Ordering::Relaxed);
    now
}

/// The highest number of bytes in use since [`start`].
pub fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

/// The golden file the input is built from.
const WALLS_AND_SLAB: &[u8] = include_bytes!("../golden/walls-and-slab.ifcx");

/// The golden file `walls-and-slab.ifcx` with the attribute `id` of the
/// first node that has one changed by `edit`, written compact.
fn with_attribute(id: &str, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut file: Value = serde_json::from_slice(WALLS_AND_SLAB).expect("the golden file is JSON");
    let node = file["data"]
        .as_array_mut()
        .expect("a data list")
        .iter_mut()
        .find(|n| n["attributes"].get(id).is_some())
        .unwrap_or_else(|| panic!("no node with {id}"));
    edit(&mut node["attributes"][id]);
    serde_json::to_vec(&file).expect("JSON serialises")
}

/// Points along a helix, `count` of them, each coordinate rounded to six
/// decimals as the writer rounds them.
fn helix(count: usize) -> impl Iterator<Item = [f64; 3]> {
    (0..count).map(|i| {
        let t = i as f64 * 0.001_234;
        let round = |v: f64| (v * 1e6).round() / 1e6;
        [
            round(10.0 + 5.0 * t.cos()),
            round(10.0 + 5.0 * t.sin()),
            round(t * 0.01),
        ]
    })
}

/// The golden file with its first body mesh (`usd::usdgeom::mesh`) replaced
/// by one of `points` points and as many triangles, valid against its
/// schema: an attribute of about 50 bytes of text per point, whose JSON tree
/// takes about 18 times its text.
pub fn large_mesh(points: usize) -> Vec<u8> {
    let indices: Vec<usize> = (0..points)
        .flat_map(|i| [i, (i + 1) % points, (i + 2) % points])
        .collect();
    with_attribute("usd::usdgeom::mesh", |mesh| {
        *mesh = json!({ "points": helix(points).collect::<Vec<_>>(), "faceVertexIndices": indices });
    })
}

/// The input of archicraft-4rrc: the golden file `walls-and-slab.ifcx` with
/// `slabs` more slabs on its ground storey, each a copy of SL-001 with a new
/// path and an outline of 999 points (a comb of 249 teeth, valid and just
/// below the point limit of the time, 1,000). The fuzz finding used 400 slabs
/// (3.3 MB); saved with their meshes they make a file of about 52 MB.
pub fn many_slabs(slabs: usize) -> Vec<u8> {
    let mut file: Value = serde_json::from_slice(WALLS_AND_SLAB).expect("the golden file is JSON");
    let data = file["data"].as_array_mut().expect("a data list");
    let named = |data: &[Value], name: &str| {
        data.iter()
            .position(|n| n["attributes"]["bsi::ifc::prop::Name"] == name)
            .unwrap_or_else(|| panic!("no node {name}"))
    };
    let storey = named(data, "00 Ground floor");
    let template = data[named(data, "SL-001")].clone();
    let mut outline: Vec<Value> = (0..249)
        .flat_map(|i| {
            let x = 2 * i;
            [json!([x, 0]), json!([x, 1]), json!([x + 1, 1]), json!([x + 1, 0])]
        })
        .collect();
    outline.extend([json!([498, 0]), json!([498, -1]), json!([0, -1])]);
    for k in 0..slabs {
        let mut slab = template.clone();
        let path = Uuid::from_u128(k as u128 + 1).to_string();
        slab["path"] = json!(path);
        slab.as_object_mut().expect("a node").remove("children");
        slab["attributes"]["archicraft::slab"]["outline"] = Value::Array(outline.clone());
        data[storey]["children"][format!("Slab_{k:08x}")] = json!(path);
        data.push(slab);
    }
    serde_json::to_vec(&file).expect("JSON serialises")
}
