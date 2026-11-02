#!/usr/bin/env python3
"""Build an isolated allocation-counting server; never instrument the production tree."""
import argparse
import io
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PROBE = r'''
mod allocation_probe {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    static CALLS: AtomicU64 = AtomicU64::new(0);
    static BYTES: AtomicU64 = AtomicU64::new(0);
    struct Counter;
    unsafe impl GlobalAlloc for Counter {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 { CALLS.fetch_add(1, Relaxed); BYTES.fetch_add(l.size() as u64, Relaxed); System.alloc(l) }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 { CALLS.fetch_add(1, Relaxed); BYTES.fetch_add(l.size() as u64, Relaxed); System.alloc_zeroed(l) }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) { System.dealloc(p, l) }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { CALLS.fetch_add(1, Relaxed); BYTES.fetch_add(n as u64, Relaxed); System.realloc(p, l, n) }
    }
    #[global_allocator] static ALLOCATOR: Counter = Counter;
    pub fn stats() -> String { format!("{{\"calls\":{},\"bytes\":{}}}", CALLS.load(Relaxed), BYTES.load(Relaxed)) }
    pub fn start() { std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        eprintln!("ALLOC calls={} requested_bytes={}", CALLS.load(Relaxed), BYTES.load(Relaxed));
    }); }
}
'''


def normalize_workspace_versions(source):
    """Repair only local package versions in a historical, stale lockfile."""
    manifest = tomllib.loads((source / 'Cargo.toml').read_text())
    workspace = manifest['workspace']
    versions = {}
    for member in workspace['members']:
        package = tomllib.loads((source / member / 'Cargo.toml').read_text())['package']
        version = package['version']
        if isinstance(version, dict):
            version = workspace['package']['version']
        versions[package['name']] = version
    path = source / 'Cargo.lock'
    original = path.read_text()
    parts = original.split('[[package]]')
    for index in range(1, len(parts)):
        package = tomllib.loads('[[package]]' + parts[index])['package'][0]
        if 'source' not in package and package['name'] in versions:
            parts[index] = re.sub(r'(?m)^version = "[^"]+"',
                                  f'version = "{versions[package["name"]]}"', parts[index], count=1)
    normalized = '[[package]]'.join(parts)
    if original != normalized:
        path.write_text(normalized)
        print('Normalized local workspace package versions; registry dependencies remain locked')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--label', required=True)
    parser.add_argument('--revision', help='Git revision; omit to copy the current working tree')
    args = parser.parse_args()
    if not re.fullmatch(r'[a-zA-Z0-9_-]+', args.label):
        parser.error('label must contain only letters, numbers, underscores and hyphens')
    source = ROOT / '.benchmarks' / f'allocation-source-{args.label}'
    source.mkdir(parents=True, exist_ok=False)
    if args.revision:
        archive = subprocess.check_output(['git', 'archive', args.revision, 'Cargo.toml', 'Cargo.lock', 'crates'], cwd=ROOT)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(source, filter='data')
    else:
        for name in ['Cargo.toml', 'Cargo.lock']:
            shutil.copy2(ROOT / name, source / name)
        shutil.copytree(ROOT / 'crates', source / 'crates')
    # Shared diagnostic target directories must not reuse a newer artifact from another source copy.
    for rust_source in (source / "crates").rglob("*.rs"):
        rust_source.touch()
    normalize_workspace_versions(source)
    path = source / 'crates/backend/src/main.rs'
    text = path.read_text()
    entry = 'async fn main() -> Result<()> {'
    router = 'let app = api::create_router(store).fallback'
    if text.count(entry) != 1 or text.count(router) != 1:
        raise RuntimeError('Server entry point changed; update the isolated instrumentation adapter')
    text = text.replace(entry, entry + '\n    allocation_probe::start();')
    text = text.replace(router, 'let app = api::create_router(store).route("/__allocation-probe", axum::routing::get(|| async { allocation_probe::stats() })).fallback')
    path.write_text(text + '\n' + PROBE)
    target = ROOT / '.benchmarks' / 'allocation-target'
    subprocess.run(['cargo', 'build', '--locked', '--offline', '--release', '--manifest-path', str(source / 'Cargo.toml'), '--bin', 'backend'],
                   cwd=ROOT, env={**os.environ, 'CARGO_TARGET_DIR': str(target)}, check=True)
    destination = ROOT / '.benchmarks' / f'backend-allocations-{args.label}'
    shutil.copy2(target / 'release/backend', destination)
    print(destination)


if __name__ == '__main__':
    main()
