"""Fine L2: developer sync, baselines and retry marks at 571e04a1, on real file bytes (SHA-1 hashes).

Close to the ObjectScript: Pipeline.Run's steps, FileHash.ComputeChanges' five passes with the `seen` set,
CommitChanges, MarkFailed, MarkTestPending, StampModule, and the lifecycle's %Reload / %Compile with the stamp at
Base.cls#L1235-L1244. Environment stand-in for the compiler: a module compiles iff no server source extends the
missing class NcMissing.Base (the probes' way to break a class); a failed compile keeps the previously compiled code
(O:nc-sync-N18-compile-fail-revert: GetValue still answers "original").
"""
from __future__ import annotations

import hashlib
import re

BROKEN = b"NcMissing.Base"

CODE_MUTANTS = {
    "no-markfailed": "a failed sync sets no retry mark (Pipeline.cls#L210-L222 dropped)",
    "no-pass3": "ComputeChanges ignores retry marks (FileHash.cls#L273-L293 dropped)",
    "stamp-on-failed-compile": "the lifecycle stamps baselines even when the compile fails",
    "commit-before-compile": "sync commits the new hashes before it compiles",
    "first-sync-loads": "a sync with no baseline loads every file (the F-SYNC-2 fix; IRIS contradicts it)",
    "compile-marks-failed": "a failed compile outside sync sets retry marks (the F-SYNC-1 fix; IRIS contradicts it)",
}


def sha(b: bytes) -> str:
    return hashlib.sha1(b).hexdigest()


class Module:
    def __init__(self, name, files: dict, owned: set, tests=frozenset(), dev=True, mut=frozenset()):
        self.name = name
        self.disk = dict(files)          # relpath -> bytes
        self.owned = set(owned)
        self.tests = set(tests)
        self.dev = dev
        self.server = {}                 # relpath -> server source bytes
        self.compiled = {}               # relpath -> source compiled last
        self.compiled_ok = True
        self.rows = {}                   # relpath -> dict(hash, failed, pending)
        self.installed = False
        self.out = []
        self.mut = frozenset(mut)

    # ---- environment: the compiler
    def compile_all(self) -> bool:
        if any(BROKEN in src for src in self.server.values()):
            self.compiled_ok = False
            return False
        self.compiled = dict(self.server)
        self.compiled_ok = True
        return True

    # ---- FileHash
    def stamp_module(self):                                        # FileHash.cls#L103-L150
        for p, b in sorted(self.disk.items()):
            r = self.rows.setdefault(p, dict(hash=None, failed=0, pending=0))
            r["hash"] = sha(b)                                     # StampOneFileWithHash #L154-L170

    def has_baseline(self):
        return bool(self.rows)

    def compute_changes(self, run_tests):                          # #L228-L336
        modified, retried, seen = {}, {}, set()

        def compare(p):
            if p in seen:
                return
            seen.add(p)
            r = self.rows.get(p)
            h = sha(self.disk[p])
            if r is None or r["hash"] != h:
                modified[p] = h
        for p in sorted(self.disk):                                 # pass 1: documents present on the server
            if p in self.server:
                compare(p)
        for p in sorted(self.owned):                                # pass 2: the reverse index
            if p in self.disk:
                compare(p)
        if "no-pass3" not in self.mut:
            for p, r in sorted(self.rows.items()):                  # pass 3: retry marks
                if r["failed"] and p in self.disk and p not in modified:
                    modified[p] = sha(self.disk[p])
                    retried[p] = 1
        if run_tests:                                               # pass 4: test marks
            for p, r in sorted(self.rows.items()):
                if r["pending"] and p in self.disk and p not in modified:
                    modified[p] = sha(self.disk[p])
        return modified, retried

    # ---- Pipeline.Run (sync)
    def sync(self, run_tests=False):
        p = self.out.append
        if not self.dev:
            return "err:Sync requires dev mode."
        if not self.has_baseline():                                 # #L43-L48
            if "first-sync-loads" in self.mut:
                for q in self.owned:
                    self.server[q] = self.disk[q]
                self.compile_all()
            self.stamp_module()
            p(f"[{self.name}] Baseline established. Run sync again to detect changes.")
            return "ok"
        modified, retried = self.compute_changes(run_tests)
        for q in list(modified):                                    # SyncReportUnroutedPaths #L120-L123
            if q not in self.owned:
                del modified[q]
        if not modified:
            p(f"[{self.name}] Nothing to sync.")
            return "ok"
        if retried:
            p(f"[{self.name}] Retrying {len(retried)} file(s) from a previous failed sync.")
        if "commit-before-compile" in self.mut:
            self.commit(modified, run_tests)
        for q in sorted(modified):                                  # SyncDispatchProcessors
            self.server[q] = self.disk[q]
        if not self.compile_all():                                  # SyncCompile
            if "no-markfailed" not in self.mut:
                for q in modified:                                  # MarkFailed: rows only
                    if q in self.rows:
                        self.rows[q]["failed"] = 1
            return "err:ERROR #5373: Class 'NcMissing.Base' ... does not exist"
        self.commit(modified, run_tests)
        for q in sorted(modified):
            p(f"  Updated: {q}")
        return "ok"

    def commit(self, modified, run_tests):                           # CommitChanges #L366-L405
        for q, h in modified.items():
            r = self.rows.setdefault(q, dict(hash=None, failed=0, pending=0))
            r["hash"], r["failed"] = h, 0
            if run_tests:
                r["pending"] = 0
            elif q in self.tests:                                    # MarkTestPending
                r["pending"] = 1

    # ---- lifecycle
    def load_or_compile(self, skip_sync=False):
        """`load <root> -dev` (again) or `compile <module>`: %Reload then %Compile, stamp unless -skip-sync."""
        for q in sorted(self.owned):
            self.server[q] = self.disk[q]
        self.installed = True
        if not self.compile_all():
            if "stamp-on-failed-compile" in self.mut and self.dev and not skip_sync:
                self.stamp_module()
            if "compile-marks-failed" in self.mut:
                for q in self.owned:
                    if q in self.rows:
                        self.rows[q]["failed"] = 1
            return "err:ERROR #5373: Class 'NcMissing.Base' ... does not exist"
        if self.dev and not skip_sync:
            self.stamp_module()
        return "ok"

    def reload(self):
        self.out.append(f"[USER/{self.name}]Reload START")
        if self.dev:                                                 # no root: no-op (Base.cls#L634)
            for q in sorted(self.owned):
                self.server[q] = self.disk[q]
        return "ok"

    def edit(self, q, b: bytes):
        self.disk[q] = b

    # ---- probe-style reads
    def value(self, q):
        src = self.compiled.get(q)
        m = re.search(rb'quit "([^"]*)"', src or b"")
        return m.group(1).decode() if m else "<error>"

    def row(self, q):
        r = self.rows.get(q)
        if r is None:
            return "<no-row>"
        return f"hash:{r['hash'][:10]} syncFailed:{r['failed']} testPending:{r['pending']}"


def sync_all(mods):
    """`sync` with no module: each developer-mode module; errors are displayed, never thrown (Main.cls#L2383-L2410)."""
    res = {}
    for m in mods:
        if m.dev:
            res[m.name] = m.sync()
    return "ok", res
