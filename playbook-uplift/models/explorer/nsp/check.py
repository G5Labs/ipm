#!/usr/bin/env python3
"""Checks for the namespace-mapping and sync models (modules 07 and 09).

  python3 -I models/explorer/nsp/check.py        (from the experiment root)

Writes evidence/model-checks-nsp.json and models/explorer/nsp/REPORT.md. Parts:
 A. IRIS mapping environment (nsenv) vs IRIS probes; every environment mutant must be contradicted.
 B. Conformance: each IRIS observation the models can express, replayed on the fine L2 machines
    (ns_fine, sync_fine), plus the printed lines of the IPM commands where the probe echoed them.
 C. Exhaustive exploration of the Verus transcriptions (ns_twin, sync_twin) within stated bounds: refinement and
    well-formedness wherever the named premises hold; every violation classified by the premise it needs.
 D. Code mutants of the fine machines: each must be rejected by B or by the L1 rules on the fine machine.
 E. Transcription check: fine machine vs Verus transcription on every explored configuration (L1 view).
"""
from __future__ import annotations

import itertools
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
IPM = ROOT.parents[2] / "ipm"
sys.path.insert(0, str(HERE))

import ns_fine as F  # noqa: E402
import ns_twin as T  # noqa: E402
import nsenv  # noqa: E402
import sync_fine as SF  # noqa: E402
import sync_twin as ST  # noqa: E402
from nsenv import LIB, Iris, dir_of  # noqa: E402

OBS = {r["name"]: r for r in json.loads((ROOT / "evidence" / "iris-observations.json").read_text())["records"]}
VER = "0.10.11-SNAPSHOT"
ANSI = re.compile(r"\x1b\[[0-9;]*m|\[[0-9;]*m")


def diff(pred: dict, name: str):
    obs = OBS[name]["values"]
    def same(v, o):
        # a model status "err" stands for any IRIS error status (the message text is not modelled)
        return str(o) == str(v) or (str(v) == "err" and str(o).startswith("err:"))
    return [f"{k}: model {v!r} vs IRIS {obs.get(k, '<absent>')!r}" for k, v in pred.items() if not same(v, obs.get(k, "<absent>"))]


# ============================================================================ A. environment
def env_probes(mut=frozenset()):
    out = {}
    e = Iris(mut)
    e.add_ns("USER", code=True, zpm=VER)
    e.add_ns("NCM1")

    def look(ns, pre, kv):
        kv[pre + "Maps"] = e.maps(ns)
        kv[pre + "PkgDest"] = dir_of(e.pkg_dest(ns))
        kv[pre + "RtnDest"] = dir_of(e.rtn_dest(ns))
        kv[pre + "HasIPM"] = int(e.has_ipm(ns))
    kv = {}
    look("NCM1", "before", kv)
    F.add_package_mapping(e, "NCM1", "USER", 1)
    look("NCM1", "afterAdd", kv)
    F.activate_mappings(e, "NCM1")
    look("NCM1", "afterActivate", kv)
    out["nc-nsp-model-env-activation"] = kv
    e = Iris(mut)
    e.add_ns("USER", code=True, zpm=VER)
    e.add_ns("NCC", code=True, zpm=VER)
    for ns in ("USER", "NCC"):
        e.create(ns, "pkg", ns); e.create(ns, "rtn:ALL", ns); e.activate(ns)
    e.add_ns("NCM2")
    kv = {"nccZpm": VER, "nccPkgDest": dir_of(e.pkg_dest("NCC"))}
    look("NCM2", "none", kv)
    F.add_package_mapping(e, "NCM2", "USER", 0); look("NCM2", "pkgUser", kv)
    F.add_routine_mapping(e, "NCM2", "ALL", "NCC", 0); look("NCM2", "pkgUserRtnNcc", kv)
    F.remove_package_mapping(e, "NCM2", 0); look("NCM2", "rtnNcc", kv)
    F.add_package_mapping(e, "NCM2", "NCC", 0); F.remove_routine_mapping(e, "NCM2", "ALL", 0)
    F.add_routine_mapping(e, "NCM2", "ALL", "USER", 0); look("NCM2", "pkgNccRtnUser", kv)
    out["nc-nsp-model-env-precedence"] = kv
    e = Iris(mut)
    e.add_ns("USER", code=True, zpm=VER)
    e.add_ns("NCM3")
    F.add_routine_mapping(e, "NCM3", "ALL", "USER", 0)
    kv = {"mapsBefore": e.maps("NCM3")}
    kv["slotCallChanged"] = F.remove_routine_mapping(e, "NCM3", 1, 0)     # IPM's call shape (Main.cls#L3998)
    kv["mapsAfterSlotCall"] = e.maps("NCM3")
    F.remove_routine_mapping(e, "NCM3", "ALL", 0)
    kv["mapsAfterAllCall"] = e.maps("NCM3")
    out["nc-nsp-model-env-rtn-slot"] = kv
    e = Iris(mut)
    e.add_ns("USER", code=True, zpm=VER)
    e.create("USER", "pkg", "USER"); e.create("USER", "rtn:ALL", "USER"); e.activate("USER")
    e.add_ns("NCM4")
    kv = {"repoBefore": dir_of(e.glo_dest("NCM4"))}
    F.map_global_equivalently(e, "IPM.Repo.DefinitionD", "USER", "NCM4", 1)
    kv["repoAfterMapGlobal"] = dir_of(e.glo_dest("NCM4"))
    F.map_package_equivalently(e, "USER", "NCM4", 1)
    kv["pkgDestAfterMapPackage"] = dir_of(e.pkg_dest("NCM4"))
    kv["maps"] = e.maps("NCM4")
    kv["userRepoDest"] = dir_of(e.glo_dest("USER"))
    out["nc-nsp-model-env-repo-activation"] = kv
    # E5, E8 from the earlier native checks
    e = Iris(mut)
    e.add_ns("USER", code=True, zpm=VER); e.add_ns("NCA"); e.add_ns("%SYS")
    for k in ("pkg", "rtn:ALL"):
        e.create("USER", k, "USER"); e.create("NCA", k, "USER")
    e.activate("USER"); e.activate("NCA")
    out["nc-nsp-N9-ipmversion-mapped-snap"] = {"ncaZpmRecord": e.zpm_record("NCA") or "-", "ncaPkgDest": dir_of(e.pkg_dest("NCA"))}
    out["nc-nsp-N1-fresh-install-all-ns"] = {"sysHasIPM": int(e.has_ipm("%SYS")), "sysPkgDest": dir_of(e.pkg_dest("%SYS"))}
    return out


def part_a():
    base = env_probes()
    bad = {n: diff(p, n) for n, p in base.items()}
    kills = {}
    for m in nsenv.MUTANTS:
        mp = env_probes(frozenset({m}))
        killers = [n for n, p in mp.items() if diff(p, n)]
        conf = [n for n, b in conformance(env_mut=frozenset({m})).items() if b]
        kills[m] = {"probes": killers, "scenarios": conf}
    return bad, kills


# ============================================================================ B. conformance (namespace)
def log_lines(obs_name, header, upto="<<< status"):
    text = (ROOT / OBS[obs_name]["log"]).read_text(errors="replace").splitlines()
    for i, l in enumerate(text):
        if header in l:
            out = []
            for l2 in text[i + 1:]:
                if upto in l2:
                    break
                out.append(ANSI.sub("", l2).replace("  | ", "", 1).strip())
            return out
    return None


KEEP = ("Mapping %IPM", "Skipping", "Mapping IPM repository", "No mapping found", "Unmapping", "IPM enabled")


def keep(lines):
    return [l for l in lines if l.startswith(KEEP)]


def std_env(mut=frozenset(), env_mut=frozenset()):
    """USER: local install (installer in a fresh namespace maps %IPM and %IPM.* to USER)."""
    e = Iris(env_mut)
    e.add_ns("USER", code=True, zpm=VER)
    for k in ("pkg", "rtn:ALL"):
        e.create("USER", k, "USER")
    e.activate("USER")
    return e


def create_namespace(e, ns, caller="USER"):
    """%IPM.Test.Utils.CreateNamespace: a new namespace whose %IPM package is mapped from the caller (only)."""
    e.add_ns(ns)
    e.create(ns, "pkg", caller)
    e.activate(ns)


def ns_scenarios(mut=frozenset(), env_mut=frozenset()):
    """{observation: (predicted values, [(model lines, IRIS lines)])}."""
    R = {}
    # --- 80-82: NCC's own install under CreateNamespace's package mapping, then clean
    e = std_env(mut, env_mut)
    create_namespace(e, "NCC")
    F.installer(e, "NCC", VER, mut)
    R["nc-nsp-setup-ncc-local-install"] = ({"status": "ok", "zpm": e.zpm_record("NCC"), "maps": e.maps("NCC")}, [])
    create_namespace(e, "NCA")
    R["nc-nsp-installer-existing-pkg-mapping"] = ({
        "nccMaps": e.maps("NCC"), "nccPkgDest": dir_of(e.pkg_dest("NCC")), "nccZpmRecord": e.zpm_record("NCC"),
        "userPkgDest": dir_of(e.pkg_dest("USER")), "ncaMapsFromCreate": e.maps("NCA")}, [])
    # FixSetup: NCC deleted and re-created, every scratch namespace's %IPM package mapping dropped
    e = std_env(mut, env_mut)
    for ns in ("NCA", "NCB", "NCD", "NCE", "NCT", "NCC"):
        e.add_ns(ns)
    F.installer(e, "NCC", VER, mut)
    R["nc-nsp-setup-ncc-clean-local-install"] = ({"status": "ok", "zpm": e.zpm_record("NCC"), "maps": e.maps("NCC")}, [])
    # --- 83: N2
    e.out = []
    F.enable_map(e, "NCC", ["NCB"], mut=mut)
    kv = {"ncaDestBefore": dir_of(e.pkg_dest("NCA")), "ncbDestBefore": dir_of(e.pkg_dest("NCB"))}
    e.out = []
    F.enable_map(e, "USER", ["NCA", "NCB", "NCC"], mut=mut)
    lines = list(e.out)
    kv.update({"cmdStatus": "ok", "ncaDestAfter": dir_of(e.pkg_dest("NCA")), "ncbDestAfter": dir_of(e.pkg_dest("NCB")),
               "nccDestAfter": dir_of(e.pkg_dest("NCC")), "ncaMaps": e.maps("NCA"), "ncbMaps": e.maps("NCB"),
               "printedMappingNCA": int("Mapping %IPM package in USER equivalently to NCA" in lines),
               "printedMappingNCB": int("Mapping %IPM package in USER equivalently to NCB" in lines),
               "printedSkipNCC": next((l for l in lines if "Skipping IPM mapping of NCC" in l), ""),
               "ncaHasIPM": int(e.has_ipm("NCA"))})
    kv["ncbStillOnNCC"] = int(kv["ncbDestAfter"] == kv["ncbDestBefore"])
    R["nc-nsp-N2-enable-map"] = (kv, [(keep(lines), keep(log_lines("nc-nsp-N2-enable-map", '>>> zpm "enable -map -ns NCA,NCB,NCC"') or []))])
    # --- N3
    e.rows["NCA"] = ["nc-nsp-own"]
    e.rows["USER"] = ["nc-nsp-user"]
    e.out = []
    F.enable_map(e, "USER", ["NCA", "NCB", "NCD", "NCE"], repos=True, mut=mut)
    lines = list(e.out)
    line = lambda s: next((l for l in lines if s in l), "")  # noqa: E731
    kv = {"cmdStatus": "ok", "skipNCA": line("repository mapping of NCA"), "skipNCB": line("repository mapping of NCB"),
          "lineNCD": line("repository mapping of NCD"), "ncaMaps": e.maps("NCA"), "ncbMaps": e.maps("NCB"),
          "ncdMaps": e.maps("NCD"), "ncaRepos": " ".join(e.repo_rows("NCA")) or "<none>",
          "ncdRepos": " ".join(e.repo_rows("NCD")) or "<none>"}
    R["nc-nsp-N3-enable-map-repos"] = (kv, [(keep(lines), keep(log_lines("nc-nsp-N3-enable-map-repos", '>>> zpm "enable -map -repos -ns NCA,NCB,NCD,NCE"') or []))])
    e.out = []
    F.enable_map(e, "USER", ["NCD", "NCE"], repos=True, mut=mut)
    lines = list(e.out)
    kv = {"cmdStatus": "ok", "lineNCD": next((l for l in lines if "repository mapping of NCD" in l), ""),
          "ncdMaps": e.maps("NCD"), "nceMaps": e.maps("NCE"), "ncdRepos": " ".join(e.repo_rows("NCD")) or "<none>",
          "ncdRepoGlobalDest": dir_of(e.glo_dest("NCD"))}
    R["nc-nsp-N3-enable-map-repos-rerun"] = (kv, [(keep(lines), keep(log_lines("nc-nsp-N3-enable-map-repos-rerun", '>>> zpm "enable -map -repos -ns NCD,NCE"') or []))])
    # --- N9 (version read only; the semantic-version rule is not modelled)
    v_nca, v_user = F.check_ipm_version(e, "NCA"), F.check_ipm_version(e, "USER")
    R["nc-nsp-N9-ipmversion-mapped-snap"] = ({"ncaPkgDest": dir_of(e.pkg_dest("NCA")), "ncaZpmRecord": e.zpm_record("NCA") or "-",
        "ncaLoad": f"err:ERROR #5001: The module requires IPM version >:0.10.11-SNAPSHOT. Current IPM version is {v_nca or '.0.0'}"}, [])
    R["nc-nsp-N9-ipmversion-mapped"] = ({"ncaPkgDest": dir_of(e.pkg_dest("NCA")), "ncaZpmRecord": e.zpm_record("NCA") or "-",
        "ncaLoad": f"err:ERROR #5001: The module requires IPM version >:0.9. Current IPM version is {v_nca or '.0.0'}",
        "userLoad": f"err:ERROR #5001: The module requires IPM version >:0.9. Current IPM version is {v_user}"}, [])
    # --- 85: N5 (registry path, install fails; the probe first adds the Remote repo nc-nsp-stub in USER), N7, N6
    e.rows["USER"] = ["nc-nsp-stub", "nc-nsp-user"]
    e.out = []
    before = int(e.has_ipm("NCT"))
    F.enable_copy_registry(e, "NCT", install_ok=False)
    R["nc-nsp-N5-enable-remote-broken"] = ({"cmdStatus": "ok", "enabledLine": "IPM enabled for namespace NCT",
                                            "nctHasIPMBefore": before, "nctHasIPMAfter": int(e.has_ipm("NCT"))}, [])
    kv = {"ncdMapsBefore": e.maps("NCD"), "ncdDestBefore": dir_of(e.pkg_dest("NCD"))}
    st = F.install_zpm(e, "NCD", VER, mut)
    kv["installZpm"] = st
    kv["zpmRecordAfterInstall"] = e.zpm_record("NCD") or "-"
    R["nc-nsp-N7-install-zpm-mapped"] = (kv, [])
    F.installer(e, "NCD", VER, mut)
    R["nc-nsp-N7-installer-in-mapped"] = ({"installerStatus": "ok", "ncdMapsAfter": e.maps("NCD"), "ncdDestAfter": dir_of(e.pkg_dest("NCD")),
        "ncdRepoGlobalDestAfter": dir_of(e.glo_dest("NCD")), "ncdZpmRecord": e.zpm_record("NCD"), "userZpmRecord": e.zpm_record("USER"),
        "userHasIPM": int(e.has_ipm("USER")), "ncdRepos": " ".join(sorted(e.repo_rows("NCD"))) or "<none>"}, [])
    e.out = []
    F.unmap(e, ["NCD"], mut=mut)
    lines = list(e.out)
    R["nc-nsp-N7-unmap-after-local-install"] = ({"unmapStatus": "ok", "noMappingFound": int("No mapping found for NCD. Skipping." in lines),
        "ncdMapsAfterUnmap": e.maps("NCD"), "unmapLine": next((l for l in lines if "NCD" in l), "")}, [])
    kv = {"nceMapsBefore": e.maps("NCE"), "ncaMapsBefore": e.maps("NCA")}
    e.out = []
    F.unmap(e, ["NCE", "NCA"], mut=mut)
    lines = list(e.out)
    kv.update({"cmdStatus": "ok", "nceMapsAfter": e.maps("NCE"), "ncaMapsAfter": e.maps("NCA"), "nceHasIPM": int(e.has_ipm("NCE")),
               "ncaHasIPM": int(e.has_ipm("NCA")), "ncaOwnRepoGlobalStill": dir_of(e.glo_dest("NCA"))})
    R["nc-nsp-N6-unmap-mapped"] = (kv, [(keep(lines), keep(log_lines("nc-nsp-N6-unmap-mapped", '>>> zpm "unmap -ns NCE,NCA"') or []))])
    kv = {"userMapsBefore": e.maps("USER")}
    e.out = []
    F.unmap(e, ["USER"], mut=mut)
    kv.update({"cmdStatus": "ok", "noMappingFound": int("No mapping found for USER. Skipping." in e.out), "userMapsAfter": e.maps("USER"),
               "userHasIPM": int(e.has_ipm("USER"))})
    R["nc-nsp-N6-unmap-source"] = (kv, [])
    for ns in ("NCA", "NCE"):
        cfg = e.cfg[ns]
        cpf = " , ".join(([f"Routine_%IPM.*:{cfg['rtn:ALL']}"] if "rtn:ALL" in cfg else []) + ([f"Package_%IPM:{cfg['pkg']}"] if "pkg" in cfg else []))
        R[f"nc-nsp-N6-unmap-recheck-{ns.lower()}"] = ({"maps": e.maps(ns), "pkgDest": dir_of(e.pkg_dest(ns)), "rtnDest": dir_of(e.rtn_dest(ns)),
                                                       "hasIPM": int(e.has_ipm(ns)), "cpf": cpf or "<none>"}, [])
    R["nc-nsp-N6-shell-after-unmap"] = ({"ns": "NCA", "status": "ok" if e.has_ipm("NCA") else "err"}, [])
    # --- 87: N4 (the repo lock held by another job: the first two steps fail; the map step's mapping work is done)
    e.add_ns("%SYS")
    e.rows["USER"] = ["nc-nsp-stub", "nc-nsp-user"]
    before = " ".join(e.rows["USER"])
    e.out = []
    F.community(e, "USER", fail=(True, True, False), mut=mut)
    lines = list(e.out)
    R["nc-nsp-N4-community-step-fails"] = ({"cmdStatus": "ok", "reposBefore": before, "reposAfter": " ".join(e.repo_rows("USER")),
                                            "nctMaps": e.maps("NCT"), "sysMaps": e.maps("%SYS")},
                                           [(keep(lines), keep(log_lines("nc-nsp-N4-community-step-fails", 'RUNNING command: "enable -map -repos -globally"', "Done.") or []))])
    # --- 88: the installer under a package-only mapping (NCX made by CreateNamespace)
    e2 = std_env(mut, env_mut)
    create_namespace(e2, "NCX")
    kv = {"ncxMapsBefore": e2.maps("NCX"), "ncxPkgDestBefore": dir_of(e2.pkg_dest("NCX")), "cpfBefore": "Package_%IPM:USER"}
    F.installer(e2, "NCX", VER, mut)
    c = e2.cfg["NCX"]
    kv.update({"installerStatus": "ok", "ncxMapsAfter": e2.maps("NCX"), "ncxPkgDestAfter": dir_of(e2.pkg_dest("NCX")),
               "ncxRtnDestAfter": dir_of(e2.rtn_dest("NCX")), "ncxZpmRecord": e2.zpm_record("NCX"),
               "cpfAfter": f"Routine_%IPM.*:{c.get('rtn:ALL')} , Package_%IPM:{c.get('pkg')}"})
    R["nc-nsp-installer-repoint"] = (kv, [])
    # --- 89: fresh container, installer in USER
    e3 = Iris(env_mut)
    e3.add_ns("USER"); e3.add_ns("%SYS")
    F.installer(e3, "USER", VER, mut)
    pk = " ".join(f"{n}:{e3.cfg[n]['pkg']}" for n in e3.namespaces() if "pkg" in e3.cfg[n])
    rt = " ".join(f"{n}:{e3.cfg[n]['rtn:ALL']}" for n in e3.namespaces() if "rtn:ALL" in e3.cfg[n])
    R["nc-nsp-N1-fresh-install-all-ns"] = ({"pkgMappings": pk, "rtnMappings": rt, "sysPkgDest": dir_of(e3.pkg_dest("%SYS")),
        "userPkgDest": dir_of(e3.pkg_dest("USER")), "sysHasIPM": int(e3.has_ipm("%SYS")), "userZpm": e3.zpm_record("USER"),
        "userRepoRows": len(e3.repo_rows("USER"))}, [])
    # --- model probes: USER-VERIFY (a zpm record, %IPM from USER's database) sorts after USER
    e4 = std_env(mut, env_mut)
    create_namespace(e4, "USER-VERIFY")
    e4.zpm["USER-VERIFY"] = VER
    create_namespace(e4, "NCM5")
    kv = {"mapsBefore": e4.maps("NCM5"), "pkgDestBefore": dir_of(e4.pkg_dest("NCM5")), "userRepos": " ".join(e4.repo_rows("USER")) or "<none>"}
    e4.out = []
    F.enable_map(e4, "USER", ["NCM5"], repos=True, mut=mut)
    kv.update({"cmdStatus": "ok", "skipLine": next((l for l in e4.out if "repository mapping of NCM5" in l), ""), "mapsAfter": e4.maps("NCM5"),
               "repoDestAfter": dir_of(e4.glo_dest("NCM5")), "reposAfter": " ".join(e4.repo_rows("NCM5")) or "<none>"})
    R["nc-nsp-model-repos-premapped"] = (kv, [])
    create_namespace(e4, "NCM6")
    kv = {"mapsBefore": e4.maps("NCM6"), "hasIPMBefore": int(e4.has_ipm("NCM6"))}
    e4.out = []
    F.unmap(e4, ["NCM6"], mut=mut)
    kv.update({"cmdStatus": "ok", "noMapping": int("No mapping found for NCM6. Skipping." in e4.out), "mapsAfter": e4.maps("NCM6"),
               "pkgDestAfter": dir_of(e4.pkg_dest("NCM6")), "hasIPMAfter": int(e4.has_ipm("NCM6"))})
    R["nc-nsp-model-unmap-package-only"] = (kv, [])
    # --- model probe: GetMapInfo on the container's state as observed (destinations and records)
    o = OBS["nc-nsp-model-mapinfo"]["values"]
    e5 = Iris(env_mut)
    srcs = set(o["sources"].split())
    dests = dict(x.split(":") for x in o["dests"].split())
    dbname = lambda d: {"user": "USER", "irislib": LIB}.get(d, d)  # noqa: E731
    for n, d in dests.items():
        e5.add_ns(n)
    for n, d in dests.items():
        db = dbname(d)
        if db != LIB:
            e5.code.add(db)
            e5.create(n, "pkg", db); e5.create(n, "rtn:ALL", db); e5.activate(n)
        if n in srcs:
            e5.zpm[n] = VER
    info = F.get_map_info(e5, mut)
    R["nc-nsp-model-mapinfo"] = ({"isMappedFrom": " ".join(f"{k}>{v}" for k, v in sorted(info.items()))}, [])
    # --- 81-nsp-fix-b3 (review finding 8, NSP-B3): the installer in a new namespace while others exist: local
    # installs (USER, NCC), a package-mapped one (NCD) and ones with no IPM; no other namespace changes.
    e6 = std_env(mut, env_mut)
    for ns in ("NCA", "NCB", "NCE", "NCT"):
        e6.add_ns(ns)
    create_namespace(e6, "NCD")
    e6.add_ns("NCC")
    F.installer(e6, "NCC", VER, mut)
    e6.add_ns("NCFRESH3")

    def snap6():
        return {n: (e6.maps(n), e6.pkg_dest(n), e6.rtn_dest(n), e6.glo_dest(n)) for n in e6.namespaces()}
    before, mb, hb = snap6(), e6.maps("NCFRESH3"), int(e6.has_ipm("NCFRESH3"))
    st = F.installer(e6, "NCFRESH3", VER, mut)
    after = snap6()
    changed = [n for n in before if n != "NCFRESH3" and after[n] != before[n]]
    R["nc-nsp-fix-B3-fresh-install-many-ns"] = ({
        "installerStatus": st, "targetMapsBefore": mb, "targetHasIPMBefore": hb, "targetMapsAfter": e6.maps("NCFRESH3"),
        "targetPkgDestAfter": dir_of(e6.pkg_dest("NCFRESH3")), "targetZpm": e6.zpm_record("NCFRESH3"),
        "otherNamespacesChanged": "none" if not changed else " ".join(changed),
        "userSnapUnchanged": int(before["USER"] == after["USER"])}, [])
    return R


def conformance(mut=frozenset(), env_mut=frozenset(), sync_mut=frozenset()):
    res = {}
    for name, (pred, line_pairs) in ns_scenarios(mut, env_mut).items():
        bad = diff(pred, name)
        for model_l, iris_l in line_pairs:
            if model_l != iris_l:
                bad.append(f"printed lines: model {model_l} vs IRIS {iris_l}")
        res[name] = bad
    for name, pred in sync_scenarios(sync_mut).items():
        res[name] = diff(pred, name)
    return res


# ============================================================================ B. conformance (sync)
DATA = IPM / "tests/integration_tests/Test/PM/Integration/_data"
SUPER = "src/cls/SyncTest/SuperClass.cls"
SUB = "src/cls/SyncTest/SubClass.cls"


def sync_test(mut, dev=True):
    files = {SUPER: (DATA / "sync-test" / SUPER).read_bytes(), SUB: (DATA / "sync-test" / SUB).read_bytes()}
    return SF.Module("sync-test", files, owned=set(files), dev=dev, mut=mut)


def dep_c(mut, dev=True):
    p = "src/cls/SyncDepC/Base.cls"
    return SF.Module("sync-dep-c", {p: (DATA / "sync-dep/c" / p).read_bytes()}, owned={p}, dev=dev, mut=mut), p


def broken(b: bytes) -> bytes:
    return b.replace(b"Class SyncTest.SuperClass\n", b"Class SyncTest.SuperClass Extends NcMissing.Base\n")


def super_of(m):
    return "NcMissing.Base" if SF.BROKEN in m.server.get(SUPER, b"") else "<none>"


def state_of(m):
    return "compiled" if m.compiled_ok else "defined"


def source_of(m):
    t = m.server.get(SUPER, b"")
    return "edited" if b'"edited"' in t else ("original" if b'"original"' in t else "other")


def sync_scenarios(mut=frozenset()):
    R = {}
    orig = (DATA / "sync-test" / SUPER).read_bytes()
    # N18
    m = sync_test(mut)
    kv = {"loadDev": m.load_or_compile()}
    kv["valueAfterLoad"] = m.value(SUPER); kv["rowAfterLoad"] = m.row(SUPER)
    m.edit(SUPER, broken(orig)); kv["brokenShaDiffers"] = int(SF.sha(m.disk[SUPER]) != SF.sha(orig))
    st = m.load_or_compile(); kv["compile"] = st[:3] if st.startswith("err") else st
    kv.update(superAfterCompile=super_of(m), stateAfterCompile=state_of(m), rowAfterCompile=m.row(SUPER))
    m.edit(SUPER, orig); kv["restoredExactBytes"] = 1
    m.out = []; kv["sync"] = m.sync()
    kv["nothingToSync"] = int(any("Nothing to sync" in l for l in m.out)); kv["updatedLine"] = next((l.strip() for l in m.out if "Updated:" in l), "")
    kv.update(superAfterSync=super_of(m), stateAfterSync=state_of(m), valueAfterSync=m.value(SUPER), rowAfterSync=m.row(SUPER))
    R["nc-sync-N18-compile-fail-revert"] = kv
    # N19
    m = sync_test(mut)
    kv = {"loadDevSkipSync": m.load_or_compile(skip_sync=True), "hasBaseline": int(m.has_baseline())}
    m.edit(SUPER, orig.replace(b'"original"', b'"edited"'))
    m.out = []; kv["sync1"] = m.sync(); kv["sync1Baseline"] = int(any("Baseline established" in l for l in m.out)); kv["valueAfterSync1"] = m.value(SUPER)
    m.out = []; kv["sync2"] = m.sync(); kv["sync2Nothing"] = int(any("Nothing to sync" in l for l in m.out)); kv["valueAfterSync2"] = m.value(SUPER)
    kv["diskHasEdited"] = int(b'"edited"' in m.disk[SUPER])
    R["nc-sync-N19-skip-sync-first-sync"] = kv
    # N20
    m = sync_test(mut); c, cp = dep_c(mut)
    kv = {"loads": f"{m.load_or_compile()} / {c.load_or_compile()}"}
    m.edit(SUPER, broken(orig)); c.edit(cp, c.disk[cp].replace(b'"c-original"', b'"c-edited"'))
    st, _ = SF.sync_all([m, c])
    kv.update(syncAll=st, depCValue=c.value(cp), syncTestRow=m.row(SUPER))
    s1 = m.sync(); kv["syncOne"] = "err" if s1.startswith("err") else s1
    R["nc-sync-N20-sync-all-one-fails"] = kv
    # N20 re-capture (review finding 8): the summary's error lines (module order is not modelled)
    m = sync_test(mut); c, cp = dep_c(mut)
    kv = {"loads": f"{m.load_or_compile()} / {c.load_or_compile()}"}
    m.edit(SUPER, broken(orig)); c.edit(cp, c.disk[cp].replace(b'"c-original"', b'"c-edited"'))
    st, res = SF.sync_all([m, c])
    failed = [n for n in (m.name, c.name) if res[n].startswith("err")]
    kv.update(syncAll=st, depCValue=c.value(cp), syncTestRow=m.row(SUPER), summaryErrors="Errors:    " + ", ".join(failed),
              summaryFailedModuleLine=" ".join(f"{n}   ERROR" for n in failed))
    R["nc-nsp-fix-sync-N20-summary"] = kv
    # N21
    c, cp = dep_c(mut, dev=False)
    kv = {"load": c.load_or_compile(), "record": "1.0.0"}
    c.edit(cp, c.disk[cp].replace(b'"c-original"', b'"c-reloaded"'))
    kv["reload"] = c.reload()
    kv["serverSourceHasReloaded"] = int(b"c-reloaded" in c.server[cp]); kv["value"] = c.value(cp)
    R["nc-sync-N21-reload-non-dev"] = kv
    # N22
    m = sync_test(mut)
    kv = {"loadDev": m.load_or_compile()}
    m.edit(SUPER, broken(orig))
    s = m.sync(); kv["brokenSync"] = "err" if s.startswith("err") else s
    kv["rowAfterFailedSync"] = m.row(SUPER)
    m.edit(SUPER, orig.replace(b'"original"', b'"fixed"'))
    kv["compile"] = m.load_or_compile(); kv["valueAfterCompile"] = m.value(SUPER); kv["rowAfterCompile"] = m.row(SUPER)
    m.out = []; kv["sync"] = m.sync()
    kv["retryLine"] = next((l for l in m.out if "Retrying" in l), ""); kv["updatedLine"] = next((l.strip() for l in m.out if "Updated:" in l), "")
    kv["rowAfterSync"] = m.row(SUPER)
    m.out = []; m.sync(); kv["syncAgainNothing"] = int(any("Nothing to sync" in l for l in m.out))
    R["nc-sync-N22-retry-mark-after-compile"] = kv
    # model probes
    m = sync_test(mut)
    kv = {"loadDev": m.load_or_compile()}
    m.edit(SUPER, orig.replace(b'"original"', b'"edited"'))
    kv["reload"] = m.reload(); kv["sourceAfterReload"] = source_of(m); kv["rowAfterReload"] = m.row(SUPER)
    m.edit(SUPER, orig); kv["restoredExactBytes"] = 1
    m.out = []; kv["sync"] = m.sync(); kv["nothingToSync"] = int(any("Nothing to sync" in l for l in m.out))
    kv["sourceAfterSync"] = source_of(m); kv["rowAfterSync"] = m.row(SUPER)
    R["nc-nsp-model-sync-reload-revert"] = kv
    m = sync_test(mut)
    kv = {"loadDev": m.load_or_compile()}
    m.edit(SUPER, orig.replace(b'"original"', b'"edited"'))
    kv["loadSkipSync"] = m.load_or_compile(skip_sync=True); kv["valueAfterLoad"] = m.value(SUPER); kv["rowAfterLoad"] = m.row(SUPER)
    m.edit(SUPER, orig); kv["restoredExactBytes"] = 1
    m.out = []; kv["sync"] = m.sync(); kv["nothingToSync"] = int(any("Nothing to sync" in l for l in m.out))
    kv.update(valueAfterSync=m.value(SUPER), sourceAfterSync=source_of(m), rowAfterSync=m.row(SUPER))
    R["nc-nsp-model-sync-skipsync-revert"] = kv
    return R


# observations of modules 07-09 the models do not express, with the reason
NOT_EXPRESSED = {
    "nc-nsp-N1-fresh-install": "same facts as -all-ns, read before the second namespace listing; repo list output not modelled",
    "nc-nsp-N8-sysns-dev": "system requirements and developer-mode load belong to the load model (01), not this one",
    "nc-nsp-N10-version-build": "display of build metadata by `version`/`info` is not modelled",
    "nc-nsp-migrate-repo-defs-success": "legacy ZPM migration is not modelled",
    "nc-nsp-ipm-test-run-1-assert-status": "IPM test tallies", "nc-nsp-ipm-test-run-4-assert-status": "IPM test tallies",
    "nc-nsp-ipm-test-run-5-assert-status": "IPM test tallies",
}
NOT_EXPRESSED_PREFIX = {"nc-res-": "module 08 (resource processors, packaging, exec, run-from-file) has no model"}


# ============================================================================ C/E. exploration (namespace)
NAMES = ["NA", "NB", "NC"]


def to_fine(s, env_mut=frozenset()):
    e = Iris(env_mut)
    nm = lambda d: LIB if d == T.LIB else NAMES[d]  # noqa: E731
    for n in sorted(s):
        e.add_ns(NAMES[n], code=s[n]["code"], zpm=str(s[n]["rec"]) if s[n]["rec"] is not None else None,
                 rows=["r"] if s[n]["rows"] else None)
    for n in sorted(s):
        pkg, rtn, repo = s[n]["act"]
        if pkg is not None:
            e.create(NAMES[n], "pkg", nm(pkg))
        if rtn is not None:
            e.create(NAMES[n], "rtn:ALL", nm(rtn))
        if repo is not None:
            for g in nsenv.REPO_GLOBALS:
                e.create(NAMES[n], "glo:" + g, nm(repo))
        e.activate(NAMES[n])
    return e


def fine_view(e):
    idx = lambda d: T.LIB if d == LIB else NAMES.index(d)  # noqa: E731
    out = {}
    for name in e.namespaces():
        n = NAMES.index(name)
        out[n] = dict(code=idx(e.pkg_dest(name)) if e.has_ipm(name) else None, repos=idx(e.glo_dest(name)),
                      own_rows=bool(e.rows.get(name)), ver=int(e.zpm[name]) if name in e.zpm else None)
    return out


def fine_consistent(e):
    return all(e.cfg[n] == e.act[n] for n in e.namespaces())


def states_n2():
    ids = [0, 1]
    per = {}
    for n in ids:
        other = [x for x in ids if x != n]
        per[n] = [T.ns((p, r, rp), (p, r, rp), c, v, w)
                  for p in [None] + ids for r in [None] + ids for rp in [None] + other
                  for c in (False, True) for v in (None, 10) for w in (False, True)]
    for a in per[0]:
        for b in per[1]:
            s = {0: a, 1: b}
            if T.wf(s):
                yield s


def shapes(n, ids):
    other = [x for x in ids if x != n]
    own = (n, n, None)
    out = [T.ns((None, None, None), (None, None, None), False, None, False),
           T.ns(own, own, True, 10, True), T.ns(own, own, True, 10, False)]
    for x in other:
        for m, c, v, w in [((x, x, None), False, None, False), ((x, x, x), False, None, False), ((x, None, None), False, None, False),
                           ((None, x, None), False, None, False), ((x, x, None), False, None, True), ((x, x, None), False, 10, False),
                           ((x, n, None), True, 10, False)]:
            out.append(T.ns(m, m, c, v, w))
    return out


def states_n3():
    ids = [0, 1, 2]
    for a, b, c in itertools.product(shapes(0, ids), shapes(1, ids), shapes(2, ids)):
        s = {0: a, 1: b, 2: c}
        if T.wf(s):
            yield s


def subsets(ids):
    return [set(c) for k in range(1, len(ids) + 1) for c in itertools.combinations(ids, k)]


def ns_ops(s):
    ids = sorted(s)
    for src in ids:
        for ts in subsets(ids):
            for repos in (False, True):
                yield ("enable", src, frozenset(ts), repos)
        yield ("community", src)
    for ts in subsets(ids):
        for ro in (False, True):
            yield ("unmap", frozenset(ts), ro)
    for n in ids:
        yield ("installer", n)
        yield ("install_zpm", n)
        yield ("version", n)


def twin_step(s, op):
    k = op[0]
    if k == "enable":
        return T.enable_map(s, op[1], op[2], op[3])
    if k == "community":
        return T.community(s, op[1])
    if k == "unmap":
        return T.unmap(s, op[1], op[2])
    if k == "installer":
        return T.installer(s, op[1], 10)
    if k == "install_zpm":
        return T.install_zpm(s, op[1], 10)
    return s


def fine_run(s, op, mut=frozenset(), order=sorted):
    e = to_fine(s)
    k = op[0]
    if k == "enable":
        F.enable_map(e, NAMES[op[1]], [NAMES[t] for t in order(op[2])], repos=op[3], quiet=True, mut=mut)
    elif k == "community":
        F.community(e, NAMES[op[1]], mut=mut)
    elif k == "unmap":
        F.unmap(e, [NAMES[t] for t in order(op[1])], op[2], mut=mut)
    elif k == "installer":
        F.installer(e, NAMES[op[1]], "10", mut=mut)
    elif k == "install_zpm":
        F.install_zpm(e, NAMES[op[1]], "10", mut=mut)
    return e


def premises(s, op):
    """Names of the premises of op's refinement lemma that fail in s."""
    k = op[0]
    bad = set()
    if k == "enable":
        src, ts, repos = op[1:]
        for name, ok in (("SRC", T.premise_src(s, src)), ("UNIQ", T.premise_uniq(s, src)), ("TGT", T.premise_tgt(s, src, ts)),
                         ("REPOS", T.premise_repos(s, ts, repos))):
            if not ok:
                bad.add(name)
    elif k == "community":
        src = op[1]
        r = T.reset_repos(s, src)
        for name, ok in (("SRC", T.premise_src(s, src)), ("UNIQ", T.premise_uniq(r, src)), ("TGT", T.premise_tgt(r, src, set(s))),
                         ("REPOS", T.premise_repos(r, set(s), True))):
            if not ok:
                bad.add(name)
    elif k == "unmap":
        ts, ro = op[1:]
        if not T.premise_info(s, ts):
            bad.add("INFO")
        if not T.premise_rtn(s, ts, ro):
            bad.add("RTN")
    elif k == "installer":
        if not T.premise_inst(s, op[1]):
            bad.add("INST")
    elif k == "install_zpm":
        if not T.listed(s, op[1]):
            bad.add("LISTED")
    elif k == "version":
        if not (T.has_ipm(s, op[1]) and T.premise_own(s, op[1])):
            bad.add("OWN")
    return frozenset(bad)


def l1_holds(s, op, post_view):
    a = T.abs_(s)
    k = op[0]
    if k == "enable":
        return post_view == T.l1_enable(a, op[1], set(op[2]), op[3])
    if k == "community":
        return post_view == T.l1_community(a, op[1])
    if k == "unmap":
        return post_view == T.l1_unmap(a, set(op[1]), op[2])
    if k in ("installer", "install_zpm"):
        return T.l1_install(a, post_view, op[1], 10)
    return True


def version_holds(s, op):
    return T.check_ipm_version(s, op[1]) == T.version_in_use(T.abs_(s), op[1])


def explore_ns(states, fine=True, mut=frozenset(), limit_examples=6):
    stat = defaultdict(lambda: {"runs": 0, "premises_hold": 0, "viol_under_premises": 0, "wf_broken_under_premises": 0,
                                "viol_by_premise": Counter(), "examples": []})
    trans = {"configs": 0, "mismatch": [], "order_mismatch": 0}
    fine_viol = Counter()
    for s in states:
        for op in ns_ops(s):
            k = op[0]
            st = stat[k]
            st["runs"] += 1
            prem = premises(s, op)
            if k == "version":
                ok = version_holds(s, op)
                post = s
            else:
                post = twin_step(s, op)
                ok = l1_holds(s, op, T.abs_(post))
            if not prem:
                st["premises_hold"] += 1
                if not ok:
                    st["viol_under_premises"] += 1
                if k != "version" and not T.wf(post):
                    st["wf_broken_under_premises"] += 1
            elif not ok:
                st["viol_by_premise"][",".join(sorted(prem))] += 1
                if len(st["examples"]) < limit_examples:
                    st["examples"].append({"op": str(op), "premises_failing": sorted(prem)})
            if fine and k != "version":
                e = fine_run(s, op, mut)
                fv = fine_view(e)
                trans["configs"] += 1
                if fv != T.abs_(post) or not fine_consistent(e):
                    if len(trans["mismatch"]) < 20:
                        trans["mismatch"].append({"op": str(op), "state": repr(s), "fine": repr(fv), "twin": repr(T.abs_(post))})
                    trans.setdefault("n_mismatch", 0)
                    trans["n_mismatch"] = trans.get("n_mismatch", 0) + 1
                if k in ("enable", "unmap") and len(op[1 if k == "unmap" else 2]) > 1:
                    e2 = fine_run(s, op, mut, order=lambda x: sorted(x, reverse=True))
                    if fine_view(e2) != fv:
                        trans["order_mismatch"] += 1
                if not prem and not (l1_holds(s, op, fv)):
                    fine_viol[k] += 1
    for v in stat.values():
        v["viol_by_premise"] = dict(v["viol_by_premise"])
    return dict(stat), trans, dict(fine_viol)


# ============================================================================ C/E. exploration (sync)
def sync_init_states(n_files=2):
    """After `load -dev` (baseline = disk = server) or `load -dev -skip-sync` (no baseline); file 1 owned or not, test or not."""
    for skip in (False, True):
        for kinds in itertools.product([(True, False), (True, True), (False, False)], repeat=n_files - 1):
            files = {0: dict(disk=1, server=1, hash=None if skip else 1, failed=False, pending=False, owned=True, test=False)}
            for i, (own, tst) in enumerate(kinds, 1):
                files[i] = dict(disk=1, server=1 if own else None, hash=None if skip else 1, failed=False, pending=False, owned=own, test=tst)
            yield {"files": files, "dev": True}


def sync_ops(m):
    ps = sorted(m["files"])
    for p in ps:
        for b in (1, 2, 3):
            if m["files"][p]["disk"] != b:
                yield ("edit", p, b)
    det = [p for p in ps if ST.detected(m["files"][p], False)]
    for rt in (False, True):
        yield ("sync", rt, True, frozenset())
        for k in range(len(det) + 1):
            for c in itertools.combinations(det, k):
                yield ("sync", rt, False, frozenset(c))
    owned = [p for p in ps if m["files"][p]["owned"]]
    for skip in (False, True):
        yield ("compile", True, skip, frozenset())
        for k in range(len(owned) + 1):
            for c in itertools.combinations(owned, k):
                yield ("compile", False, skip, frozenset(c))
    yield ("reload",)


def sync_step(m, op):
    k = op[0]
    if k == "edit":
        return ST.edit(m, op[1], op[2]), None
    if k == "sync":
        return ST.sync_state(m, op[1], op[2], op[3]), ST.sync_ok(m, op[1], op[2], True)
    if k == "compile":
        return ST.compile_dev(m, op[1], op[2], op[3]), None
    return ST.reload(m), None


def sync_premise(m, op):
    k = op[0]
    if k == "sync":
        return "BASE" if not ST.premise_base(m) else None
    if k == "compile":
        return "OUT" if not ST.premise_out(m, op[1], op[2], op[3]) else None
    if k == "reload":
        return "OUT" if not ST.premise_out_reload(m) else None
    return None


def sync_l1(m, op, post, ok):
    a, b = ST.abs_(m), ST.abs_(post)
    k = op[0]
    if k == "edit":
        return ST.l1_edit(a, b, op[1], op[2])
    if k == "sync":
        return ST.l1_sync(a, b, ok)
    return ST.l1_load(a, b)


def key(m):
    return json.dumps(m, sort_keys=True, default=str)


def explore_sync(depth=4):
    seen, frontier = set(), []
    for m in sync_init_states():
        seen.add(key(m)); frontier.append((m, True))
    stat = {"states": 0, "transitions": 0, "premise_and_inv_ok": 0, "viol_under_premises": 0, "inv_broken_under_premises": 0,
            "defect_steps_by_premise": Counter(), "success_mismatch_traces_all_premises": 0, "success_mismatch_after_premise_failure": 0,
            "outcome_checks": 0, "outcome_violations": 0, "examples": []}
    for _ in range(depth):
        nxt = []
        for m, clean in frontier:
            for op in sync_ops(m):
                post, ok = sync_step(m, op)
                stat["transitions"] += 1
                if op[0] == "sync":
                    # L1 outcome, both directions, for both test results and for the same state outside developer mode
                    for mm in (m, dict(m, dev=False)):
                        for tests_ok in (True, False):
                            stat["outcome_checks"] += 1
                            rep_ = ST.sync_ok(mm, op[1], op[2], tests_ok)
                            if not ST.l1_outcome(mm["dev"], ST.sync_ran(mm, op[1]), op[2], op[1], tests_ok, rep_):
                                stat["outcome_violations"] += 1
                bad = sync_premise(m, op)
                l1 = sync_l1(m, op, post, ok)
                if bad is None and ST.inv(m):
                    stat["premise_and_inv_ok"] += 1
                    if not l1:
                        stat["viol_under_premises"] += 1
                    if not ST.inv(post):
                        stat["inv_broken_under_premises"] += 1
                elif bad is not None and (not l1 or (ST.inv(m) and not ST.inv(post))):
                    stat["defect_steps_by_premise"][bad] += 1
                    if len(stat["examples"]) < 4:
                        stat["examples"].append(f"{op} with premise {bad} failing")
                c2 = clean and bad is None
                if op[0] == "sync" and ok and not l1:
                    if c2:
                        stat["success_mismatch_traces_all_premises"] += 1
                    else:
                        stat["success_mismatch_after_premise_failure"] += 1
                kk = key(post)
                if kk not in seen:
                    seen.add(kk); nxt.append((post, c2))
        frontier = nxt
    stat["states"] = len(seen)
    stat["defect_steps_by_premise"] = dict(stat["defect_steps_by_premise"])
    return stat


def sync_fine_traces(mut=frozenset(), depth=4):
    """Fine machine on real bytes: every trace of edits (original, edited, broken) and commands up to depth;
    returns (L1 violations where the Verus premises held, transcription mismatches, runs)."""
    orig = (DATA / "sync-test" / SUPER).read_bytes()
    contents = [orig, orig.replace(b'"original"', b'"edited"'), broken(orig)]
    ids = {}

    def iid(b):
        return ids.setdefault(b, len(ids) + 1) if b is not None else None
    ops = [("edit", c) for c in range(3)] + [("sync",), ("sync_test",), ("compile",), ("compile_skip",), ("reload",)]
    viol, mism, runs = 0, [], 0
    for skip0 in (False, True):
        for trace in itertools.product(ops, repeat=depth):
            m = sync_test(mut)
            m.load_or_compile(skip_sync=skip0)
            tw = twin_of(m, iid)
            clean = True
            for op in trace:
                runs += 1
                before = tw
                if op[0] == "edit":
                    m.edit(SUPER, contents[op[1]])
                    tw_op = ("edit", 0, iid(contents[op[1]]))
                    post, ok = sync_step(before, tw_op)
                elif op[0] in ("sync", "sync_test"):
                    rt = op[0] == "sync_test"
                    det = frozenset(p for p, f in before["files"].items() if ST.detected(f, rt))
                    st = m.sync(run_tests=rt)
                    tw_op = ("sync", rt, not st.startswith("err") or not det, det)
                    post, ok = sync_step(before, tw_op)
                    if st.startswith("err") and det:
                        ok = False
                else:
                    skip = op[0] == "compile_skip"
                    if op[0] == "reload":
                        m.reload(); tw_op = ("reload",)
                    else:
                        st = m.load_or_compile(skip_sync=skip)
                        tw_op = ("compile", not st.startswith("err"), skip, frozenset(before["files"]))
                    post, ok = sync_step(before, tw_op)
                bad = sync_premise(before, tw_op)
                clean = clean and bad is None
                fine_now = twin_of(m, iid)
                if fine_now != post:
                    if len(mism) < 10:
                        mism.append({"trace": str(trace), "op": str(tw_op), "fine": repr(fine_now), "twin": repr(post)})
                if clean and tw_op[0] == "sync" and ok:
                    if any(f["owned"] and f["server"] != f["disk"] for f in fine_now["files"].values()):
                        viol += 1
                tw = fine_now
    return viol, mism, runs


KNOWN = {}


def twin_of(m, iid):
    idx = {SUPER: 0, SUB: 1}
    for b in list(m.disk.values()) + list(m.server.values()):
        KNOWN[SF.sha(b)] = b
    by_hash = KNOWN
    files = {}
    for p, i in idx.items():
        r = m.rows.get(p)
        files[i] = dict(disk=iid(m.disk[p]), server=iid(m.server.get(p)), hash=iid(by_hash[r["hash"]]) if r and r["hash"] in by_hash else (None if not r else -1),
                        failed=bool(r and r["failed"]), pending=bool(r and r["pending"]), owned=p in m.owned, test=p in m.tests)
    return {"files": files, "dev": m.dev}


# ============================================================================ main
def main():
    checks, rep = [], ["# Explorer report: namespace mapping and sync (generated by models/explorer/nsp/check.py)", ""]

    # A
    bad, kills = part_a()
    ok_a = all(not b for b in bad.values())
    checks.append({"name": "nsp-env-contract-differential", "status": "pass" if ok_a else "fail", "detail": {n: (b or "match") for n, b in bad.items()}})
    all_k = all(k["probes"] or k["scenarios"] for k in kills.values())
    checks.append({"name": "nsp-env-mutants-killed", "status": "pass" if all_k else "fail", "detail": kills})
    rep += ["## A. IRIS mapping environment vs IRIS", "", f"Probes compared: {len(bad)}; mismatches: {sum(len(b) for b in bad.values())}.", ""]
    rep += [f"- `{n}`: {'match' if not b else '; '.join(b)}" for n, b in bad.items()]
    rep += ["", "Environment mutants:", ""]
    rep += [f"- `{m}` ({nsenv.MUTANTS[m]}): {'KILLED by ' + ', '.join(k['probes'] + k['scenarios'][:3]) if (k['probes'] or k['scenarios']) else 'SURVIVED'}" for m, k in kills.items()]
    rep.append("")

    # B
    conf = conformance()
    for n, b in conf.items():
        checks.append({"name": f"nsp-conf-{n}", "status": "pass" if not b else "fail", "detail": b or "match"})
    ok_b = all(not b for b in conf.values())
    checks.append({"name": "nsp-trace-conformance", "status": "pass" if ok_b else "fail", "detail": f"{len(conf)} observations"})
    mine = [n for n in OBS if n.startswith(("nc-nsp", "nc-res", "nc-sync"))]
    missing = [n for n in mine if n not in conf and n not in bad and n not in NOT_EXPRESSED and not n.startswith(tuple(NOT_EXPRESSED_PREFIX))]
    if missing:
        raise SystemExit(f"observations neither replayed nor listed as not expressed: {missing}")
    not_expr = {n: NOT_EXPRESSED.get(n) or next(v for p, v in NOT_EXPRESSED_PREFIX.items() if n.startswith(p))
                for n in mine if n not in conf and n not in bad}
    rep += ["## B. Fine machines vs IRIS observations", "", f"Observations replayed: {len(conf)}; failing: {sum(1 for b in conf.values() if b)}.", ""]
    rep += [f"- `{n}`: {'match' if not b else '; '.join(b)}" for n, b in conf.items()]
    rep += ["", f"Not expressed ({len(not_expr)}):", ""] + [f"- `{n}`: {why}" for n, why in sorted(not_expr.items())] + [""]
    checks.append({"name": "nsp-observations-not-expressed", "status": "info", "detail": not_expr})

    # C + E (namespace)
    s2, t2, fv2 = explore_ns(list(states_n2()))
    s3, t3, fv3 = explore_ns(list(states_n3()))
    rep += ["## C. Exploration of the Verus transcriptions", "",
            "Bounds: (a) 2 namespaces, every combination of package/routine/repo mapping, code, zpm record and registry rows "
            "(only well-formed states); (b) 3 namespaces, each one of 17 shapes (none, local with/without registries, and for each "
            "other namespace: mapped, mapped with registries, package-only, routine-only, mapped with own registries, installer in a "
            "mapped namespace, local install under a package mapping). Every command on every subset of targets.", ""]
    rep.append("| Bound | Command | Runs | Premises hold | Violations under premises | wf broken | Violations by failing premise |")
    rep.append("|---|---|---|---|---|---|---|")
    for bound, S in (("2 ns", s2), ("3 ns", s3)):
        for k, v in sorted(S.items()):
            rep.append(f"| {bound} | {k} | {v['runs']} | {v['premises_hold']} | {v['viol_under_premises']} | {v['wf_broken_under_premises']} | {v['viol_by_premise']} |")
    rep.append("")
    for k in sorted(set(s2) | set(s3)):
        a, b = s2.get(k, {}), s3.get(k, {})
        ok = a.get("viol_under_premises", 0) == 0 and b.get("viol_under_premises", 0) == 0 and \
            a.get("wf_broken_under_premises", 0) == 0 and b.get("wf_broken_under_premises", 0) == 0
        checks.append({"name": f"nsp-explore-{k}", "status": "pass" if ok else "fail",
                       "detail": {"2ns": {x: a.get(x) for x in ("runs", "premises_hold", "viol_under_premises", "wf_broken_under_premises", "viol_by_premise")},
                                  "3ns": {x: b.get(x) for x in ("runs", "premises_hold", "viol_under_premises", "wf_broken_under_premises", "viol_by_premise")},
                                  "examples": (a.get("examples", []) + b.get("examples", []))[:6]},
                       "conditions": "exhaustive within the bounds; a pass means the L1 effect holds wherever the lemma's premises hold"})
    defects = {"REPOS": "F-NSP-4", "UNIQ": "F-NSP-7", "INST": "F-NSP-5", "RTN": "F-NSP-6", "OWN": "F-NSP-2"}
    for prem, fid in defects.items():
        n = sum(c for S in (s2, s3) for v in S.values() for p, c in v["viol_by_premise"].items() if prem in p.split(","))
        checks.append({"name": f"nsp-explore-witness-{fid}", "status": "pass" if n > 0 else "fail",
                       "detail": {"violations_needing_premise": prem, "count": n},
                       "conditions": "the defect shows up in exploration, and only where its premise fails"})
    trans_ok = not t2.get("n_mismatch") and not t3.get("n_mismatch") and t2["order_mismatch"] == 0 and t3["order_mismatch"] == 0
    checks.append({"name": "nsp-transcription-check", "status": "pass" if trans_ok else "fail",
                   "detail": {"configs": t2["configs"] + t3["configs"], "mismatches": t2.get("n_mismatch", 0) + t3.get("n_mismatch", 0),
                              "list_order_mismatches": t2["order_mismatch"] + t3["order_mismatch"], "first": (t2["mismatch"] + t3["mismatch"])[:5]},
                   "conditions": "fine machine (ordered list loops, per-call activation) vs Verus transcription, L1 view, every explored configuration; target list in both orders"})
    rep += ["## E. Fine machine vs Verus transcription", "",
            f"Configurations: {t2['configs'] + t3['configs']}; view mismatches: {t2.get('n_mismatch', 0) + t3.get('n_mismatch', 0)}; "
            f"target-list order changes the result: {t2['order_mismatch'] + t3['order_mismatch']} times.", ""]
    if t2["mismatch"] or t3["mismatch"]:
        rep += [f"- {x}" for x in (t2["mismatch"] + t3["mismatch"])[:5]] + [""]

    # C (sync)
    ss = explore_sync()
    ok_s = (ss["viol_under_premises"] == 0 and ss["inv_broken_under_premises"] == 0 and ss["success_mismatch_traces_all_premises"] == 0
            and ss["outcome_checks"] > 0 and ss["outcome_violations"] == 0)
    checks.append({"name": "sync-explore-refinement", "status": "pass" if ok_s else "fail", "detail": ss,
                   "conditions": "files: one owned class plus one owned, owned test or unowned file; bytes 1-3; all syncs (both outcomes, every loaded subset, with and without -test; the reported result checked against the two-way L1 outcome for both test results and outside developer mode), compiles (both outcomes, every loaded subset, with and without -skip-sync), reloads and edits; depth 4 from load -dev and load -dev -skip-sync"})
    for prem, fid in (("OUT", "F-SYNC-1"), ("BASE", "F-SYNC-2")):
        checks.append({"name": f"sync-explore-witness-{fid}", "status": "pass" if ss["defect_steps_by_premise"].get(prem, 0) > 0 else "fail",
                       "detail": {"premise": prem, "steps_breaking_L1_or_invariant": ss["defect_steps_by_premise"].get(prem, 0),
                                  "later_successful_syncs_with_stale_server": ss["success_mismatch_after_premise_failure"]}})
    fv, fm, fr = sync_fine_traces()
    checks.append({"name": "sync-fine-refinement", "status": "pass" if fv == 0 else "fail", "detail": {"runs": fr, "violations": fv},
                   "conditions": "fine machine on the real SuperClass bytes (original, edited, broken), every trace of 4 commands from load -dev and from load -dev -skip-sync; a successful sync on a trace whose premises all held leaves every owned file's server source equal to disk"})
    checks.append({"name": "sync-transcription-check", "status": "pass" if not fm else "fail", "detail": {"runs": fr, "mismatches": fm[:5]},
                   "conditions": "fine machine vs Verus transcription after every command of every trace"})
    rep += ["## C. Exploration of the sync transcription", "", f"States: {ss['states']}; transitions: {ss['transitions']}; premises and invariant held: {ss['premise_and_inv_ok']}; "
            f"L1 violations there: {ss['viol_under_premises']}; invariant broken there: {ss['inv_broken_under_premises']}; "
            f"steps breaking L1 or the invariant, by failing premise: {ss['defect_steps_by_premise']}; successful syncs leaving a stale server copy: "
            f"{ss['success_mismatch_traces_all_premises']} on traces where every premise held, {ss['success_mismatch_after_premise_failure']} after a premise failed. "
            f"Reported sync result vs the two-way L1 outcome (both test results, and outside developer mode): "
            f"{ss['outcome_checks']} checks, {ss['outcome_violations']} violations.", "",
            f"Fine sync machine on real bytes: {fr} commands; L1 violations on premise-respecting traces: {fv}; transcription mismatches: {len(fm)}.", ""]

    # D
    base_bad = {n for n, b in conf.items() if b}
    mres = {}
    st3 = list(states_n3())
    for m in F.CODE_MUTANTS:
        cb = [n for n, b in conformance(mut=frozenset({m})).items() if b and n not in base_bad]
        _, _, fv_m = explore_ns(st3, fine=True, mut=frozenset({m}))
        mres[m] = {"conformance_failures": cb, "fine_l1_violations_under_premises": fv_m}
    for m in SF.CODE_MUTANTS:
        cb = [n for n, b in conformance(sync_mut=frozenset({m})).items() if b and n not in base_bad]
        v, _, _ = sync_fine_traces(frozenset({m}), depth=3)
        mres["sync:" + m] = {"conformance_failures": cb, "fine_l1_violations_under_premises": v}
    rejected = all(r["conformance_failures"] or r["fine_l1_violations_under_premises"] for r in mres.values())
    checks.append({"name": "nsp-code-mutants-rejected", "status": "pass" if rejected else "fail", "detail": mres})
    rep += ["## D. Code mutants of the fine machines", ""]
    allm = dict(F.CODE_MUTANTS, **{"sync:" + k: v for k, v in SF.CODE_MUTANTS.items()})
    for m, r in mres.items():
        verdict = "REJECTED" if (r["conformance_failures"] or r["fine_l1_violations_under_premises"]) else "ACCEPTED (control failed)"
        rep.append(f"- `{m}` ({allm[m]}): {verdict}; IRIS observations contradicting it: {len(r['conformance_failures'])}; "
                   f"L1 violations under premises: {r['fine_l1_violations_under_premises']}")
    rep.append("")

    (ROOT / "evidence" / "model-checks-nsp.json").write_text(json.dumps({"tool": "models/explorer/nsp/check.py", "records": checks}, indent=2, default=str) + "\n")
    (HERE / "REPORT.md").write_text("\n".join(rep) + "\n")
    print("\n".join(rep))
    fails = [c["name"] for c in checks if c["status"] == "fail"]
    print("FAILING:", fails or "none")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
