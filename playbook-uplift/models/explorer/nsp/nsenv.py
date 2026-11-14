"""Executable IRIS mapping environment contract (playbook 3.2) for the namespace model.

Facts (each validated against IRIS 2026.1 observations in check.py, part A):
 E1  GetPackageDest(ns, "%IPM"): the active %IPM.* routine mapping wins, then the %IPM package mapping, else IRISLIB.
 E1' GetRoutineDest(ns, "%IPM.Main.1"): the same database (a package mapping also maps the package's routines).
 E2  Config.Map* changes are seen by lookups only after activation of the namespace (ActivateMappings / batch).
 E3  GetGlobalDest(ns, "IPM.Repo.DefinitionD"): the active global mapping, else the namespace's own database.
 E3' AddGlobalMapping(ns, name, db, pSetCollation, pSkipActivation): activates at once unless its 5th argument is set.
 E4  %Installer <ClassMapping>/<RoutineMapping> create a mapping only when none exists, and activate.
 E5  A zpm module record is namespace data; a package mapping does not carry it.
 E6  IPM is enabled in ns (%IPM.Main:Shell, %IPM.Storage.Module defined) iff its %IPM database holds IPM code.
 E7  Routine mappings are keyed by name plus "_type" unless the type is ALL; "%IPM.*_1" is never created.
 E8  IRISLIB holds no IPM code.

MUTANTS switch one fact off; each must be contradicted by an observation.
"""
from __future__ import annotations

LIB = "IRISLIB"
REPO_GLOBALS = ("IPM.Repo.DefinitionD", "IPM.Repo.DefinitionS", "IPM.Repo.DefinitionI")

MUTANTS = {
    "cfg-is-active": "a created mapping is seen at once, without activation (E2 off)",
    "pkg-wins": "the %IPM package mapping wins over the %IPM.* routine mapping (E1 reversed)",
    "rtn-dest-no-pkg": "GetRoutineDest ignores the package mapping (E1' off)",
    "records-follow-package": "a package mapping also carries the zpm record (E5 off)",
    "routine-type-ignored": "RemoveRoutineMapping ignores the type argument (E7 off)",
    "global-map-honours-skip": "AddGlobalMapping defers activation when a 1 is passed in 4th position (E3' off)",
    "installer-repoints": "%Installer re-points an existing mapping (E4 off)",
    "lib-has-ipm": "IRISLIB holds IPM code (E8 off)",
}


class Iris:
    def __init__(self, mut=frozenset()):
        self.mut = frozenset(mut)
        self.cfg = {}        # ns -> {"pkg": db, "rtn:<type>": db, "glo:<name>": db}
        self.act = {}
        self.code = set()    # databases holding IPM code
        self.zpm = {}        # ns -> version string (record in the namespace's own data)
        self.rows = {}       # db -> list of registry names (^IPM.Repo.DefinitionD subnodes)
        self.out = []        # terminal output lines

    def add_ns(self, name, code=False, zpm=None, rows=None):
        self.cfg.setdefault(name, {})
        self.act.setdefault(name, {})
        if code:
            self.code.add(name)
        if zpm:
            self.zpm[name] = zpm
        if rows:
            self.rows[name] = list(rows)

    def namespaces(self):
        return sorted(self.cfg)   # %SYS.Namespace_List / $order: name order

    # ---- Config.Map* (cfg) and activation
    def exists(self, ns, key):
        return key in self.cfg[ns]

    def create(self, ns, key, db):
        self.cfg[ns][key] = db
        if "cfg-is-active" in self.mut:
            self.act[ns] = dict(self.cfg[ns])

    def delete(self, ns, key):
        self.cfg[ns].pop(key, None)
        if "cfg-is-active" in self.mut:
            self.act[ns] = dict(self.cfg[ns])

    def activate(self, ns):
        self.act[ns] = dict(self.cfg[ns])

    # ---- lookups (active table)
    def pkg_dest(self, ns):
        a = self.act[ns]
        if "pkg-wins" in self.mut:
            return a.get("pkg") or a.get("rtn:ALL") or LIB
        return a.get("rtn:ALL") or a.get("pkg") or LIB

    def rtn_dest(self, ns):
        a = self.act[ns]
        if "rtn-dest-no-pkg" in self.mut:
            return a.get("rtn:ALL") or LIB
        return self.pkg_dest(ns)

    def glo_dest(self, ns, g="IPM.Repo.DefinitionD"):
        return self.act[ns].get("glo:" + g) or ns

    def holds_code(self, db):
        return db in self.code or (db == LIB and "lib-has-ipm" in self.mut)

    def has_ipm(self, ns):
        return self.holds_code(self.pkg_dest(ns))

    def zpm_record(self, ns):
        if ns in self.zpm:
            return self.zpm[ns]
        if "records-follow-package" in self.mut:
            d = self.act[ns].get("pkg")
            return self.zpm.get(d) if d else None
        return None

    def repo_rows(self, ns):
        return self.rows.get(self.glo_dest(ns), [])

    # ---- the probe helper Maps(): "pkg:<db> rtn:<db> repoD:<db>"
    def maps(self, ns):
        c = self.cfg[ns]
        return f"pkg:{c.get('pkg', '-')} rtn:{c.get('rtn:ALL', '-')} repoD:{c.get('glo:IPM.Repo.DefinitionD', '-')}"


def dir_of(db):
    """GetPackageDest's directory form as the probes print it."""
    return "^/usr/irissys/mgr/irislib/" if db == LIB else ("^/usr/irissys/mgr/user/" if db == "USER" else f"^/usr/irissys/mgr/{db}/")
