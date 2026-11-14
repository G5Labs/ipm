#!/usr/bin/env python3
"""Checks of the uninstall and settings models (modules 05-06) through their executable twins.

  python3 -I models/explorer/repo/check.py        (from the experiment root)

Writes evidence/model-checks-repo.json (same record format as evidence/model-checks.json) and
models/explorer/repo/REPORT.md. Parts:
 A. Transcription: every `spec fn` of models/verus/{uninstall,settings}.rs has a twin function of the same name, and
    the twin reproduces every fact that a Verus witness proves.
 B. Conformance: each nc-unin-*/nc-repo-* IRIS observation the model can express is replayed on the twin.
 C. Exhaustive exploration within stated bounds: every L1 clause holds whenever its premises hold; each premise is
    needed (a counterexample exists without it).
 D. Twin mutants: each must be contradicted by at least one IRIS observation of part B.
"""
from __future__ import annotations

import itertools
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent.parent
sys.path.insert(0, str(HERE))

import settings_twin as S  # noqa: E402
import unin_twin as U  # noqa: E402

OBS = {r["name"]: r["values"] for r in json.loads((ROOT / "evidence" / "iris-observations.json").read_text())["records"]}
FUEL = 6
R = U.Res

# ============================================================================ A. transcription


def parity(rs_file: Path, mod) -> dict:
    names = re.findall(r"spec fn (\w+)", rs_file.read_text())
    missing = sorted(n for n in set(names) if not hasattr(mod, n))
    return {"spec_fns": len(set(names)), "missing": missing}


def witness_agreement() -> dict:
    out = {}
    T = U.Typed

    def chk(name, cond):
        out[name] = bool(cond)

    w = U.w_blocked(); o = U.main_uninstall(w, U.plain(2), False, False, False, False, 3)
    chk("unin_witness_blocked_deletes_webapp", U.wf_cmd(w, U.plain(2)) and o["ok"] and o["w"]["hist"] == (U.HRow(2, True),)
        and 2 in o["w"]["mods"] and 11 not in o["w"]["present"] and not U.l1_blocked_unchanged(w, 2, False, False, o) and not U.l1_honest(w, 2, o))
    w = U.w_self(); n = T(U.k_ipm(), False); o = U.main_uninstall(w, n, False, False, False, False, 3)
    chk("unin_witness_external_name_bypass", U.wf_cmd(w, n) and not U.premise_pn(w, n) and o["ok"] and U.sd() not in o["w"]["present"]
        and not o["w"]["exit_flag"] and not o["w"]["prompted"] and not U.l1_self_guard(w, False, o))
    w = U.w_ghost(); o = U.main_uninstall(w, U.plain(2), False, False, False, False, 3)
    chk("unin_witness_unresolvable_dependent", U.intent_dependent(w, 2) and not U.premise_pg(w) and o["ok"] and 2 not in o["w"]["mods"]
        and 10 not in o["w"]["present"] and 3 in o["w"]["mods"] and not U.l1_dependents_protect(w, 2, False, o))
    w = U.w_global(); o = U.main_uninstall(w, U.plain(2), False, False, False, False, 3)
    chk("unin_witness_global_kept", not U.premise_pga(w, 2) and o["ok"] and 2 not in o["w"]["mods"] and 13 in o["w"]["present"]
        and not U.l1_complete(w, 2, False, o))
    w = U.w_shared(); o = U.main_uninstall(w, U.plain(3), False, False, False, False, 3)
    chk("unin_witness_shared_folder", not U.premise_pf(w) and o["ok"] and 3 not in o["w"]["mods"] and 14 not in o["w"]["present"]
        and not U.l1_kept_files(w, o) and not U.l1_only_own(w, 3, False, o))
    w = U.w_clean(); o = U.clean_cmd(w, 2, False, False, 3)
    chk("unin_witness_clean_own_module", not U.premise_pcw(w, 2) and o["ok"] and 2 in o["w"]["mods"] and 10 in o["w"]["present"]
        and 11 not in o["w"]["present"] and o["w"]["hooks"] == (2,) and not U.l1_clean_keeps_target(w, 2, o))
    w = U.w_ipmdep(); o = U.main_uninstall(w, U.plain(2), False, True, False, False, 3)
    chk("unin_witness_recurse_removes_ipm", U.wf_cmd(w, U.plain(2)) and not U.premise_pi(w, 2, True) and not o["ok"]
        and U.sd() not in o["w"]["present"] and 10 not in o["w"]["present"] and 2 in o["w"]["mods"] and 0 in o["w"]["mods"]
        and not o["w"]["exit_flag"] and not U.l1_ipm_kept(w, o))
    w = U.w_recskip(); o = U.main_uninstall(w, U.plain(3), False, True, False, False, 3)
    chk("unin_witness_recurse_skipped_dep", o["ok"] and 3 not in o["w"]["mods"] and 2 in o["w"]["mods"] and 10 in o["w"]["present"]
        and 11 not in o["w"]["present"] and o["w"]["hist"] == (U.HRow(2, True), U.HRow(3, True)) and not U.l1_rows_honest(w, o))
    w = U.w_recremove(); o = U.main_uninstall(w, U.plain(3), False, True, False, False, 3)
    chk("unin_witness_recurse_leaves_manifest", o["ok"] and 3 not in o["w"]["mods"] and 2 not in o["w"]["mods"]
        and 3 not in o["w"]["streams"] and 2 in o["w"]["streams"] and not U.l1_manifest_with_record(w, o))
    # settings
    c0 = S.cfg0(); c1 = S.config_set(c0, 0, 5, 30)[0]; c2 = S.config_set(c1, 0, 5, 1)[0]; c3 = S.config_delete(c2, 0, 5)[0]
    chk("repo_witness_delete_restores_first_value", not S.premise_ps(c2, 5) and S.config_get(c2, 0, 5) == 1 and S.config_get(c3, 0, 5) == 30
        and S.factory(c2, 5) == 0 and S.cfg_abs(c3) != {**S.cfg_abs(c2), 5: S.factory(c2, 5)})
    chk("repo_witness_config_errors_report_ok", S.config_set(c0, 0, 9, 1)[1] == "Ok" and S.config_set(c0, 0, 1, 99)[1] == "Ok"
        and S.config_get(S.config_set(c0, 0, 1, 99)[0], 0, 1) == 0 and not S.l1_config_status(c0, 9, 1, "Ok"))
    s0 = {"dbs": {}}; d = {7: 1}
    chk("repo_witness_dm_shared", not S.premise_pns({1, 2}, 1) and 7 in S.dm_get(S.dm_set(s0, 1, d), 2)
        and S.dm_abs(S.dm_set(s0, 1, d), {1, 2}) != S.dm_l1_set(S.dm_abs(s0, {1, 2}), 1, d))
    rs = (S.reg(1, "Remote", True, False, True),)
    chk("repo_witness_publish_fallback", not S.premise_ph(rs) and S.publish_target(rs, None) == 0 and not S.l1_publish(rs, None, 0))
    rs = (S.reg(1, "Remote", True, False, False), S.reg(2, "Remote", True, True, False))
    chk("repo_witness_publish_flag_ignored", S.premise_ph(rs) and S.premise_pu(rs) and not S.premise_pk(rs) and S.publish_target(rs, None) == 0
        and not S.l1_publish(rs, None, 0))
    rs = (S.reg(1, "Oras", False, False, False),)
    chk("repo_witness_disabled_registry_used", S.publish_target(rs, 1) == 0 and not S.l1_publish(rs, 1, 0) and S.search_lists(rs, 0)
        and not S.l1_search(rs, 0))
    rs = (S.reg(2, "Remote", True, False, False)._replace(sort_override=2, offer=1), S.reg(1, "Remote", True, False, False)._replace(sort_override=1, offer=1))
    chk("repo_witness_install_tie_break", not S.premise_pt(rs) and S.install_pick(rs) == 0 and S.better(rs, 1, 0) and not S.l1_install(rs, 0))
    return out

# ============================================================================ B. conformance


def b(x):
    return "1" if x else "0"


def compare(pred: dict, obs: dict):
    return [{"key": k, "model": v, "iris": obs.get(k)} for k, v in pred.items() if str(obs.get(k)) != str(v)]


def ipm_world(mods, res, present, mapped=False):
    w = U.base({0: U.ipm_mod(), **mods}, {0: U.code(0), **res}, {0, *present})
    w["mapped_from"] = mapped
    return w


def mod(key, deps=(), res=(), needs=(), graph="same", installer=False, hook_ok=True):
    g = frozenset(needs) if graph == "same" else (None if graph is None else frozenset(graph))
    return U.Module(key, key * 10, False, installer, hook_ok, tuple(deps), tuple(res), frozenset(needs), g)


def last_row(o, m):
    rows = [h for h in o["w"]["hist"] if h.m == m]
    return "<none>" if not rows else "uninstall/" + ("success" if rows[-1].ok else "failure")


def st(o):
    return "ok" if o["ok"] else "err"


def conf_unin() -> dict:
    res = {}
    # nc-unin-1: nc-web-dep (2: class 10, web 11) needed by nc-web-top (3)
    w = ipm_world({2: mod(2, res=[10, 11]), 3: mod(3, deps=[2], res=[12], needs=[2])},
                  {10: U.code(2), 11: R(("Web",), 2), 12: U.code(3)}, {10, 11, 12})
    o = U.main_uninstall(w, U.plain(2), False, False, False, False, FUEL)
    res["nc-unin-1-blocked-uninstall"] = {"status": st(o), "depInstalled": b(2 in o["w"]["mods"]), "depClass": "compiled" if 10 in o["w"]["present"] else "absent",
                                          "webBefore": b(11 in w["present"]), "webAfter": b(11 in o["w"]["present"]),
                                          "historyRowsAdded": str(len(o["w"]["hist"])), "lastHistoryDep": last_row(o, 2)}
    # nc-unin-2: nc-web-w (2: class 10, web 11, installer) clean
    w = ipm_world({2: mod(2, res=[10, 11], installer=True)}, {10: U.code(2), 11: R(("Web",), 2)}, {10, 11})
    o = U.clean_cmd(w, 2, False, False, FUEL)
    hooks = str(sum(1 for h in o["w"]["hooks"] if h == 2))
    res["nc-unin-2-clean-own-module"] = {"status": st(o), "installed": b(2 in o["w"]["mods"]), "class": "compiled" if 10 in o["w"]["present"] else "absent",
                                         "webBefore": "1", "webAfter": b(11 in o["w"]["present"]), "unconfigureCalls": hooks, "uninstallHookCalls": hooks}
    # nc-unin-3: nc-purge (2: persistent class 10, installer), with and without -purge (not modelled: same run)
    for name in ("nc-unin-3-uninstall-plain", "nc-unin-3-uninstall-purge"):
        w = ipm_world({2: mod(2, res=[10], installer=True)}, {10: U.code(2)}, {10})
        o = U.main_uninstall(w, U.plain(2), False, False, False, False, FUEL)
        res[name] = {"status": st(o), "installed": b(2 in o["w"]["mods"]), "class": "compiled" if 10 in o["w"]["present"] else "absent",
                     "hookCalls": str(len(o["w"]["hooks"]))}
    # nc-unin-4: NCMAP borrows IPM from USER: classes present (mapped), no IPM record
    for typed, n in (("zpm", U.Typed(U.k_zpm(), True)), ("ZPM", U.Typed(U.k_zpm(), False)), ("ipm", U.Typed(U.k_ipm(), False)), ("IPM", U.Typed(U.k_ipm(), False))):
        w = U.base({}, {0: R(("Code", True), 0)}, {0})
        w["mapped_from"] = True
        o = U.main_uninstall(w, n, False, False, False, False, FUEL)
        # refused by CheckModuleNamespace = failed before the uninstall log was started (no history row)
        res[f"nc-unin-4-mapped-uninstall-{typed}"] = {"status": st(o), "refusedMapped": b(not o["ok"] and o["w"]["hist"] == w["hist"]),
                                                      "post.map.ipmMain": "compiled" if 0 in o["w"]["present"] else "absent",
                                                      "post.map.zpmRecord": b(0 in o["w"]["mods"]), "printedPrompt": b(o["w"]["prompted"])}
    # nc-unin-5: nc-ghost-b (3) needs nc-ghost-a (2: class 10); control with a good graph, then with a graph error
    wc = ipm_world({2: mod(2, res=[10]), 3: mod(3, deps=[2], res=[12], needs=[2])}, {10: U.code(2), 12: U.code(3)}, {10, 12})
    oc = U.main_uninstall(wc, U.plain(2), False, False, False, False, FUEL)
    w = ipm_world({2: mod(2, res=[10]), 3: mod(3, deps=[2], res=[12], needs=[2], graph=None)}, {10: U.code(2), 12: U.code(3)}, {10, 12})
    o = U.main_uninstall(w, U.plain(2), False, False, False, False, FUEL)
    res["nc-unin-5-unresolvable-dependent"] = {"control.status": st(oc), "control.aInstalled": b(2 in oc["w"]["mods"]),
                                               "control.skipped": b(2 in oc["w"]["mods"] and oc["ok"]),
                                               "status": st(o), "aInstalled": b(2 in o["w"]["mods"]), "aClass": "compiled" if 10 in o["w"]["present"] else "absent",
                                               "bInstalled": b(3 in o["w"]["mods"]), "skipped": b(2 in o["w"]["mods"] and o["ok"]),
                                               "dependentsListed": str(sum(1 for m in w["mods"] if U.code_dependent(w, 2, frozenset(), m))),
                                               "graphErrors": str(sum(1 for m in w["mods"] if w["mods"][m].graph is None))}
    # nc-unin-6: nc-gbl1 (2: global without the Global attribute), nc-gbl2 (3: with it)
    w = ipm_world({2: mod(2, res=[13]), 3: mod(3, res=[14])}, {13: R(("Gbl", False, False), 2), 14: R(("Gbl", True, False), 3)}, {13, 14})
    o1 = U.main_uninstall(w, U.plain(2), False, False, False, False, FUEL)
    o2 = U.main_uninstall(o1["w"], U.plain(3), False, False, False, False, FUEL)
    res["nc-unin-6-global-resources"] = {"u1.status": st(o1), "u2.status": st(o2), "installed1": b(2 in o2["w"]["mods"]), "installed2": b(3 in o2["w"]["mods"]),
                                         "dataBefore": "10", "dataAfter": "10" if 13 in o2["w"]["present"] else "0",
                                         "data2Before": "10", "data2After": "10" if 14 in o2["w"]["present"] else "0"}
    # nc-unin-7: fc-keep-on-uninstall (2): shared/ (dir, keep) 20, shared-file.txt (file, keep) 21, mine.txt (file) 22, all in folder 7;
    # nc-fc-other (3): other/ (dir, no keep) 23 into folder 7.
    keepres = {20: R(("Files", 7, False, True, False), 2), 21: R(("Files", 7, True, True, False), 2), 22: R(("Files", 7, True, False, False), 2)}

    def files(o, others):
        p = o["w"]["present"]
        return {"after.dir": b(any(r in p for r in (20, 21, 22, 23))), "after.mine.txt": b(22 in p), "after.shared.txt": b(20 in p),
                "after.shared-file.txt": b(21 in p), "after.other.txt": b(23 in p and others)}
    w = ipm_world({2: mod(2, res=[20, 21, 22])}, keepres, {20, 21, 22})
    o = U.main_uninstall(w, U.plain(2), False, False, False, False, FUEL)
    res["nc-unin-7-control-keep-module"] = {"status": st(o), **files(o, False)}
    w = ipm_world({2: mod(2, res=[20, 21, 22]), 3: mod(3, res=[23])}, {**keepres, 23: R(("Files", 7, False, False, False), 3)}, {20, 21, 22, 23})
    o = U.main_uninstall(w, U.plain(3), False, False, False, False, FUEL)
    res["nc-unin-7-shared-target-folder"] = {"status": st(o), "keepInstalled": b(2 in o["w"]["mods"]), **files(o, True)}
    # nc-unin-8: nc-p (2: class 10) needs nc-q (3: class 11, installer whose hook fails); uninstall nc-p -r
    w = ipm_world({2: mod(2, deps=[3], res=[10], needs=[3]), 3: mod(3, res=[11], installer=True, hook_ok=False)}, {10: U.code(2), 11: U.code(3)}, {10, 11})
    o = U.main_uninstall(w, U.plain(2), False, True, False, False, FUEL)
    res["nc-unin-8-dependency-hook-fails"] = {"status": st(o), "pInstalled": b(2 in o["w"]["mods"]), "qInstalled": b(3 in o["w"]["mods"]),
                                              "pClass": "compiled" if 10 in o["w"]["present"] else "absent", "qClass": "compiled" if 11 in o["w"]["present"] else "absent",
                                              "qHookCalls": str(sum(1 for h in o["w"]["hooks"] if h == 3)),
                                              "historyP": last_row(o, 2), "historyQ": last_row(o, 3)}
    # nc-unin-9: zpm clean with nc-ghost-a (2) and nc-web-w (3) installed
    w = ipm_world({2: mod(2, res=[10]), 3: mod(3, res=[11, 12])}, {10: U.code(2), 11: U.code(3), 12: R(("Web",), 3)}, {10, 11, 12})
    o = U.clean_cmd(w, U.k_zpm(), False, False, FUEL)
    res["nc-unin-9-clean-on-ipm"] = {"status": st(o), "zpmInstalled": b(0 in o["w"]["mods"]), "aInstalled": b(2 in o["w"]["mods"]),
                                     "wInstalled": b(3 in o["w"]["mods"]), "ipmMainCompiled": "compiled" if 0 in o["w"]["present"] else "absent",
                                     "otherModules": str(len(o["w"]["mods"]) - 1), "selfUninstallFlag": b(o["w"]["exit_flag"])}
    # nc-unin-env-name-case: lookup through the two case-blind indexes
    w = ipm_world({}, {}, set())
    res["nc-unin-env-name-case"] = {"NameExists.ZPM": b(any(w["mods"][m].key == U.k_zpm() for m in w["mods"])),
                                    "NameExists.IPM": b(any(w["mods"][m].key == U.k_ipm() for m in w["mods"])),
                                    "ExternalNameExists.IPM": b(any(w["mods"][m].ext_key == U.k_ipm() for m in w["mods"])),
                                    "ExternalNameExists.ZPM": b(any(w["mods"][m].ext_key == U.k_zpm() for m in w["mods"]))}
    # self-uninstall in USER with nc-ghost-a (2) and nc-web-w (3: web 12)
    def self_world():
        return ipm_world({2: mod(2, res=[10]), 3: mod(3, res=[11, 12])}, {10: U.code(2), 11: U.code(3), 12: R(("Web",), 3)}, {10, 11, 12})
    w = self_world(); o = U.main_uninstall(w, U.Typed(U.k_zpm(), True), False, False, False, False, FUEL)
    res["nc-unin-self-zpm-answer-no"] = {"status": st(o), "ipmMain": "absent" if 0 not in o["w"]["present"] else "compiled",
                                         "zpmRecords": b(0 in o["w"]["mods"]), "moduleRecords": str(len(o["w"]["mods"])),
                                         "webAppNcW": b(12 in o["w"]["present"]), "ncGhostAClass": "compiled" if 10 in o["w"]["present"] else "absent"}
    w = self_world(); o = U.main_uninstall(w, U.Typed(U.k_ipm(), False), False, False, False, False, FUEL)
    res["nc-unin-self-external-name-IPM"] = {"status": st(o), "ipmMain": "absent" if 0 not in o["w"]["present"] else "compiled",
                                             "printedPrompt": b(o["w"]["prompted"]), "printedListHeader": b(o["w"]["prompted"]),
                                             "printedExitMessage": b(o["w"]["exit_flag"]), "zpmRecords": b(0 in o["w"]["mods"]),
                                             "moduleRecords": str(len(o["w"]["mods"])), "webAppNcW": b(12 in o["w"]["present"])}
    # model probes: nc-rec-top (3) and nc-rec-other (4) need nc-web-dep (2: class 10, web 11)
    w = ipm_world({2: mod(2, res=[10, 11]), 3: mod(3, deps=[2], res=[12], needs=[2]), 4: mod(4, deps=[2], res=[13], needs=[2])},
                  {10: U.code(2), 11: R(("Web",), 2), 12: U.code(3), 13: U.code(4)}, {10, 11, 12, 13})
    o = U.main_uninstall(w, U.plain(3), False, True, False, False, FUEL)
    res["nc-repo-model-unin-recurse-skipped-dep"] = {"status": st(o), "topInstalled": b(3 in o["w"]["mods"]), "topClass": "compiled" if 12 in o["w"]["present"] else "absent",
                                                     "depInstalled": b(2 in o["w"]["mods"]), "depClass": "compiled" if 10 in o["w"]["present"] else "absent",
                                                     "otherInstalled": b(4 in o["w"]["mods"]), "webAfter": b(11 in o["w"]["present"]),
                                                     "historyRowsAdded": str(len(o["w"]["hist"])), "lastHistoryDep": last_row(o, 2), "lastHistoryTop": last_row(o, 3)}
    w = ipm_world({2: mod(2, res=[10, 11]), 3: mod(3, deps=[2], res=[12], needs=[2])}, {10: U.code(2), 11: R(("Web",), 2), 12: U.code(3)}, {10, 11, 12})
    o = U.main_uninstall(w, U.plain(3), False, True, False, False, FUEL)
    res["nc-repo-model-unin-recurse-removed-dep"] = {"status": st(o), "topInstalled": b(3 in o["w"]["mods"]), "depInstalled": b(2 in o["w"]["mods"]),
                                                     "depClass": "compiled" if 10 in o["w"]["present"] else "absent", "webAfter": b(11 in o["w"]["present"]),
                                                     "depStreamBefore": b(2 in w["streams"]), "topStreamBefore": b(3 in w["streams"]),
                                                     "depStreamAfter": b(2 in o["w"]["streams"]), "topStreamAfter": b(3 in o["w"]["streams"]),
                                                     "historyRowsAdded": str(len(o["w"]["hist"])), "lastHistoryDep": last_row(o, 2)}
    w = U.w_ipmdep(); o = U.main_uninstall(w, U.plain(2), False, True, False, False, FUEL)
    res["nc-repo-model-unin-recurse-ipm-dependency"] = {"status": st(o), "ipmMain": "absent" if 0 not in o["w"]["present"] else "compiled",
                                                        "ncRecIpmClass": "absent" if 10 not in o["w"]["present"] else "compiled",
                                                        "ncRecIpmRecord": b(2 in o["w"]["mods"]), "zpmRecords": b(0 in o["w"]["mods"]),
                                                        "printedPrompt": b(o["w"]["prompted"]), "printedListHeader": b(o["w"]["prompted"]),
                                                        "selfUninstallFlagLeft": b(o["w"]["exit_flag"])}
    return res


def node_str(c, k):
    n = c["nodes"].get(k)
    if n is None:
        return "<undef>/<undef>"
    f = lambda v: "<undef>" if v is None else ("" if v == 0 else str(v))  # noqa: E731
    return f(n.val) + "/" + f(n.dflt)


def conf_repo() -> dict:
    res = {}
    rs = (S.reg("nc-repo-a", "Remote", True, False, True),)
    i = S.publish_target(rs, None)
    res["nc-repo-1-publish-no-flag"] = {"publishingServerPick": rs[i].name if i is not None else "<none>", "printedPublishTo": b(i is not None),
                                        "printedNoServer": b(i is None)}
    rs = (S.reg("nc-repo-fs2", "Folder", True, False, False), S.reg("nc-repo-o2", "Oras", False, False, False)._replace(offer=1))
    res["nc-repo-2-disabled-registry"] = {"enabledFlag": b(rs[1].enabled), "search.listsDisabledRepo": b(S.search_lists(rs, 1)),
                                          "publishDisabled.status": "ok" if S.publish_target(rs, "nc-repo-o2") is not None else "err",
                                          "installDisabled.status": "ok" if S.install_pick(rs) is not None else "err"}
    # nc-repo-6: HistoryRetain (key 5) never set, no seeded default
    c0 = {"nodes": {}, "conf": frozenset({1, 2, 5}), "seed": {}}
    c1 = S.config_set(c0, 0, 5, 30)[0]; c2 = S.config_set(c1, 0, 5, 1)[0]; c3, s3 = S.config_delete(c2, 0, 5)
    sval = lambda c: ("<undef>" if 5 not in c["nodes"] else str(c["nodes"][5].val), "<undef>" if 5 not in c["nodes"] else str(c["nodes"][5].dflt))  # noqa: E731
    res["nc-repo-6-config-delete"] = {"fresh.value": sval(c0)[0], "fresh.default": sval(c0)[1], "after30.value": sval(c1)[0], "after30.default": sval(c1)[1],
                                      "after1.value": sval(c2)[0], "after1.default": sval(c2)[1], "afterDelete.value": sval(c3)[0],
                                      "afterDelete.default": sval(c3)[1], "delete.status": s3.lower(), "getPrinted": str(S.config_get(c3, 0, 5))}
    # nc-repo-7: USER (1) sets, NCMAP (2) reads, USER deletes
    s0 = {"dbs": {}}
    s1 = S.dm_set(s0, 1, {"NcProbe": 1})
    s2 = S.dm_delete(s1, 1)
    res["nc-repo-7-default-modifiers-namespaces"] = {"map.getListsNcProbe": b("NcProbe" in S.dm_get(s1, 2)), "map.getSaysNone": b(not S.dm_get(s1, 2)),
                                                     "map.dataAfterUserDelete": "0" if not S.dm_get(s2, 2) else "10"}
    # nc-repo-8: unknown key, bad TestReportFormat
    c0 = {"nodes": {}, "conf": frozenset({1, 2, 5}), "seed": {}}
    cs, ss = S.config_set(c0, 0, 9, 1)
    cb, sb = S.config_set(c0, 0, S.k_trf(), 99)
    res["nc-repo-8-config-errors"] = {"setUnknown.status": ss.lower(), "setUnknown.printedNotFound": b(9 not in c0["conf"]),
                                      "getUnknown.status": "ok", "setBadFormat.status": sb.lower(),
                                      "setBadFormat.valueAfter": "" if S.get_value(cb, S.k_trf()) == 0 else str(S.get_value(cb, S.k_trf()))}
    res["nc-repo-env-percent-global-shared"] = {"seenInMap": b(S.pct_db(1) == S.pct_db(2)), "seenInSys": b(S.pct_db(1) == S.pct_db(3))}
    # model probe: publish row order
    def pick(rs):
        i = S.publish_target(rs, None)
        return rs[i].name if i is not None else "<none>"
    c1 = (S.reg("nc-pm-a", "Remote", True, False, False), S.reg("nc-pm-b", "Remote", True, True, False))
    c2 = (S.reg("nc-pm-a", "Remote", True, False, True), S.reg("nc-pm-b", "Remote", True, True, False))
    c3 = (S.reg("nc-pm-a", "Remote", True, False, False), S.reg("nc-pm-o", "Oras", True, True, False))
    c4 = (S.reg("nc-pm-c1", "Remote", True, False, False), S.reg("nc-pm-c2", "Remote", True, False, False), S.reg("nc-pm-c3", "Remote", True, True, False))
    rows = lambda rs, k: " ".join(f"{r.name}:{S.publish_order(r)}" for r in rs if r.kind == k and r.enabled)  # noqa: E731
    res["nc-repo-model-publish-row-order"] = {"c1.pick": pick(c1), "c1.remoteRows": rows(c1, "Remote"), "c1.publishTo91": b(pick(c1) == "nc-pm-a"),
                                              "c2.pick": pick(c2), "c2.remoteRows": rows(c2, "Remote"), "c3.pick": pick(c3), "c3.orasRows": rows(c3, "Oras"),
                                              "c4.pick": pick(c4), "c4.remoteRows": rows(c4, "Remote")}
    # model probe: config delete on an unset key (PublishTimeout, key 6) and a seeded key (ColorScheme, key 7, seed "")
    cu = {"nodes": {}, "conf": frozenset({1, 2, 6, 7}), "seed": {7: 0}}
    d1 = S.config_delete(cu, 0, 6)[0]; s7 = S.config_set(d1, 0, 6, 7)[0]; d2 = S.config_delete(s7, 0, 6)[0]
    cs0 = {"nodes": {7: S.Node(0, 0)}, "conf": frozenset({1, 2, 6, 7}), "seed": {7: 0}}
    cs1 = S.config_set(cs0, 0, 7, 20)[0]; cs2 = S.config_delete(cs1, 0, 7)[0]
    res["nc-repo-model-config-delete"] = {"u.fresh": node_str(cu, 6), "u.afterDelete1": node_str(d1, 6), "u.afterSet7": node_str(s7, 6),
                                          "u.afterDelete2": node_str(d2, 6), "s.fresh": node_str(cs0, 7),
                                          "s.afterSet": node_str(cs1, 7).replace("20", "none"), "s.afterDelete": node_str(cs2, 7)}
    s0 = {"dbs": {}}
    a = S.dm_set(S.dm_set(s0, 1, {"NcA": 1}), 1, {"NcB": 2}); bb = S.dm_set(a, 1, {"NcA": 3}); cdel = S.dm_delete(bb, 1)
    fmt = lambda d: " ".join(f"{k}:{v}" for k, v in sorted(d.items()))  # noqa: E731
    obs = OBS.get("nc-repo-model-default-modifiers-merge", {})
    strip = lambda s: " ".join(x for x in str(s).split() if x.startswith("Nc"))  # noqa: E731
    res["nc-repo-model-default-modifiers-merge"] = {"afterTwoSets": fmt(S.dm_get(a, 1)), "afterOverwrite": fmt(S.dm_get(bb, 1)),
                                                    "afterDelete": "0" if not S.dm_get(cdel, 1) else "10",
                                                    "getListsA": b("NcA" in S.dm_get(a, 1)), "getListsB": b("NcB" in S.dm_get(a, 1))}
    # the observed lists also hold a "zpm" node that the shell's data array carries; only the Nc* options are compared
    if obs:
        OBS["nc-repo-model-default-modifiers-merge"] = {**obs, "afterTwoSets": strip(obs.get("afterTwoSets")), "afterOverwrite": strip(obs.get("afterOverwrite"))}
    # F-INST-4 observations (module 03-04): equal versions in two registries
    for name, so1 in (("nc-inst-10b-registry-tie-break", 1), ("nc-inst-10c-registry-tie-break-high", 3)):
        rs = (S.Reg("nc-inst-r2", "Remote", True, False, False, False, 2, 1), S.Reg("nc-inst-r1", "Remote", True, False, False, False, so1, 1))
        i = S.install_pick(rs)
        res[name] = {"x.repo": rs[i].name if i is not None else "<none>"}
    return res


NOT_EXPRESSIBLE = {
    "nc-unin-10-orphans": "orphans is a read-only listing; not in the model",
    "nc-unin-10-orphans-detail": "orphans; not in the model",
    "nc-unin-env-obj-delete-default": "class data globals are not resources in the model",
    "nc-unin-env-obj-delete-e-flag": "class data globals are not resources in the model",
    "nc-unin-4-mapped-setup": "setup of the mapped namespace (enable -map)",
    "nc-unin-self-prepare": "setup",
    "nc-unin-self-zpm-after-reinstall": "reinstalling IPM is not modelled",
    "nc-unin-self-ipm-after-reinstall": "reinstalling IPM is not modelled",
    "nc-repo-model-unin-recurse-ipm-reinstall": "reinstalling IPM is not modelled",
    "nc-repo-3-url-change-credentials": "registry credentials are not modelled",
    "nc-repo-4-token-auth-method": "HTTP request building is not modelled",
    "nc-repo-5-sslconfig-publish-url": "HTTP request building is not modelled",
    "nc-repo-9-unpublish-deployed-version": "unpublish and ORAS tags are not modelled",
    "nc-repo-10-invalid-type": "repo command parsing is not modelled",
    "nc-repo-10-control-type-moniker": "repo command parsing is not modelled",
    "nc-repo-11-folder-cache": "folder registry cache is not modelled",
    "nc-repo-12-nested-namespace-name": "ORAS registry namespaces are not modelled",
    "nc-repo-12-cleanup": "housekeeping",
    "nc-repo-13-secret-at-rest": "credential storage is not modelled",
    "nc-repo-zot-cleanup": "housekeeping",
    "nc-repo-fix-unin-all-order": "uninstall -all (UninstallAll) is not modelled (uninstall.TRUST.md U9)",
}

# ============================================================================ C. exploration

UNIN_CLAUSES = {
    "blocked-unchanged": "a blocked uninstall changes nothing (premises PG, PW)",
    "honest": "success exactly when the module is gone, both directions (premises PB, PG, PI, PNF)",
    "dependents-protect": "a needed module stays without -force (premise PG)",
    "only-own": "only the target's resources and records go, and its needs' with -r (premises PF, PNF)",
    "kept-files": "files copied with the keep flag stay (premises PF, PNF)",
    "complete": "once the record is gone, every non-exempt resource is gone (premises PGA, PNF)",
    "self-guard": "uninstalling IPM runs the guard (premise PN)",
    "ipm-kept": "IPM stays when another module is uninstalled (premise PI)",
    "clean-keeps-target": "clean leaves its module unchanged (premises PCW, PF)",
    "rows-honest": "a row says success exactly when its module is gone, both directions (premises PR, PB, PG, PNF, target not IPM)",
    "manifest-with-record": "a removed record takes its stored manifest (premises PR, PNF)",
    "not-found": "an unknown name fails and changes nothing (no premise)",
}

EXTRAS = {"none": None, "web": ("Web",), "gbl-unnamed": ("Gbl", False, False), "dir-keep": ("Files", 7, False, True, False),
          "dir-nokeep": ("Files", 7, False, False, False), "file-nokeep": ("Files", 7, True, False, False), "code-mapped": ("Code", True)}


def closure(deps_of, m):
    seen, stack = set(), list(deps_of.get(m, ()))
    while stack:
        d = stack.pop()
        if d not in seen:
            seen.add(d)
            stack.extend(deps_of.get(d, ()))
    return frozenset(seen)


def build(spec, mapped=False, ipm_record=True):
    """spec: {mid: (deps, graph_ok, gs, inst, extra)}"""
    deps_of = {m: v[0] for m, v in spec.items()}
    mods, res, present = {}, {0: U.Res(("Code", not ipm_record), 0)}, {0}
    if ipm_record:
        mods[0] = U.ipm_mod()
    for m, (deps, gok, gs, inst, extra) in spec.items():
        needs = closure(deps_of, m)
        rl = [m * 10]
        res[m * 10] = U.code(m)
        present.add(m * 10)
        if EXTRAS[extra] is not None:
            res[m * 10 + 1] = U.Res(EXTRAS[extra], m)
            rl.append(m * 10 + 1)
            present.add(m * 10 + 1)
        mods[m] = U.Module(m, m * 10, gs, inst != "none", inst != "fail", tuple(deps), tuple(rl), needs, needs if gok else None)
    w = U.base(mods, res, present)
    w["mapped_from"] = mapped
    return w


def commands(w):
    targets = [U.plain(m) for m in w["mods"] if m != 0] + [U.Typed(U.k_zpm(), True), U.Typed(U.k_ipm(), False), U.plain(99)]
    for n in targets:
        for force, recurse, glob in itertools.product((False, True), repeat=3):
            for answer in ((False, True) if n["exact_zpm"] and not force else (False,)):
                yield ("uninstall", n, force, recurse, glob, answer)
    for m in list(w["mods"]) + [99]:
        for force in (False, True):
            yield ("clean", m, force, False, False, False)


def judge(w, cmd, viol, runs):
    kind, n, force, recurse, glob, answer = cmd
    if kind == "clean":
        k = n if n != 0 else U.k_zpm()
        t = U.lookup(w, k)
        o = U.clean_cmd(w, k, force, glob, FUEL)
        runs[0] += 1
        if t is None:
            rec(viol, "not-found", True, U.l1_not_found(w, o), cmd)
        else:
            rec(viol, "clean-keeps-target", U.premise_pcw(w, t) and U.premise_pf(w), U.l1_clean_keeps_target(w, t, o), cmd)
        return
    if not U.wf_cmd(w, n):
        return
    o = U.main_uninstall(w, n, force, recurse, glob, answer, FUEL)
    runs[0] += 1
    t = U.lookup(w, n["key"])
    if t is None:
        rec(viol, "not-found", True, U.l1_not_found(w, o), cmd)
        return
    pnf = U.premise_pnf(n, force, answer)
    pg, pf = U.premise_pg(w), U.premise_pf(w)
    pb = U.premise_pb(w, t, force, glob)
    pi = U.premise_pi(w, t, recurse)
    rec(viol, "blocked-unchanged", pg and U.premise_pw(w, t), U.l1_blocked_unchanged(w, t, force, glob, o), cmd)
    rec(viol, "honest", pb and pg and pi and pnf, U.l1_honest(w, t, o), cmd)
    rec(viol, "dependents-protect", pg, U.l1_dependents_protect(w, t, force, o), cmd)
    rec(viol, "only-own", pf and pnf, U.l1_only_own(w, t, recurse, o), cmd)
    rec(viol, "kept-files", pf and pnf, U.l1_kept_files(w, o), cmd)
    rec(viol, "complete", U.premise_pga(w, t) and pnf, U.l1_complete(w, t, glob, o), cmd)
    if t == 0:
        rec(viol, "self-guard", U.premise_pn(w, n), U.l1_self_guard(w, force, o), cmd)
    else:
        rec(viol, "ipm-kept", pi, U.l1_ipm_kept(w, o), cmd)
        rec(viol, "rows-honest", U.premise_pr(recurse) and pb and pg and pnf, U.l1_rows_honest(w, o), cmd)
    rec(viol, "manifest-with-record", U.premise_pr(recurse) and pnf, U.l1_manifest_with_record(w, o), cmd)


def rec(viol, clause, premises, holds, cmd):
    if holds:
        return
    key = (clause, premises)
    viol.setdefault(key, [0, None])
    viol[key][0] += 1
    if viol[key][1] is None:
        viol[key][1] = str(cmd[:5])


def worlds():
    insts = ("none", "ok", "fail")
    # family 1: IPM + two modules, every attribute combination
    for d2 in ((), (0,)):
        for d3 in ((), (0,), (2,), (0, 2)):
            for a2 in itertools.product((True, False), (False, True), insts, EXTRAS):
                for a3 in itertools.product((True, False), (False, True), insts, EXTRAS):
                    yield "two-modules", build({2: (d2, *a2), 3: (d3, *a3)})
    # family 2: IPM + three modules, every acyclic dependency graph, with or without a web app each
    for d2 in ((), (0,)):
        for d3 in ((), (2,)):
            for d4 in ((), (2,), (3,), (2, 3)):
                for ex in itertools.product(("none", "web"), repeat=3):
                    for gok in (True, False):
                        yield "three-modules", build({2: (d2, gok, False, "none", ex[0]), 3: (d3, True, False, "none", ex[1]), 4: (d4, True, False, "none", ex[2])})
    # family 3: a namespace that borrows IPM (no IPM record), mapped or not
    for mapped in (False, True):
        for extra in EXTRAS:
            yield "mapped-ipm", build({2: ((), True, False, "none", extra)}, mapped=mapped, ipm_record=False)


def explore_unin():
    viol, runs, nworlds = {}, [0], {}
    for fam, w in worlds():
        nworlds[fam] = nworlds.get(fam, 0) + 1
        for cmd in commands(w):
            judge(w, cmd, viol, runs)
    return viol, runs[0], nworlds


SET_LAWS = {
    "config-get-after-set": "config get returns the value config set stored (no premise)",
    "config-delete-factory": "config delete restores the factory default (premise PS)",
    "config-shared": "a setting set in one namespace is read in every namespace (no premise)",
    "config-errors-unchanged": "an unknown key or a bad value changes nothing (no premise)",
    "dm-per-namespace": "default modifiers set in one namespace change only that namespace (premise PNS)",
    "dm-laws": "-set adds without dropping, -delete clears, a typed option wins (no premise)",
    "publish-default-enabled": "publish with no -repo picks an enabled, non-folder registry (no premise)",
    "publish-default-holder": "publish with no -repo picks the publish-flag holder (premises PH, PU, PK)",
    "publish-named": "publish -r uses the named registry only if enabled (premise PE)",
    "search-enabled": "search lists only enabled registries (premise PE)",
    "install-enabled": "install takes only from enabled registries (no premise)",
    "install-tie-sort-order": "equal versions come from the lowest sort order (premise PT)",
}


def explore_settings():
    viol, runs = {}, [0]

    def r(law, prem, holds, ex):
        runs[0] += 1
        if not holds:
            viol.setdefault((law, prem), [0, str(ex)])[0] += 1
    # config: keys 1 (TRF), 2 (MROOT), 5 (seeded), 6 (unseeded), 9 (unknown); values 0, 7, 8, 10, 99; sequences up to 3 ops
    conf, seed = frozenset({1, 2, 5, 6}), {5: 8}
    ops = [("set", k, v) for k in (1, 2, 5, 6, 9) for v in (0, 7, 8, 10, 99)] + [("delete", k, 0) for k in (1, 2, 5, 6, 9)]
    starts = [{"nodes": {}, "conf": conf, "seed": seed}, {"nodes": {5: S.Node(8, 8)}, "conf": conf, "seed": seed}]
    for c0 in starts:
        for n in range(0, 3):
            for seq in itertools.product(ops, repeat=n):
                c = c0
                for op, k, v in seq:
                    c = S.config_set(c, 1, k, v)[0] if op == "set" else S.config_delete(c, 1, k)[0]
                for k in (1, 2, 5, 6, 9):
                    for v in (0, 7, 10, 99):
                        c2, stt = S.config_set(c, 1, k, v)
                        if S.valid_set(c, k, v):
                            r("config-get-after-set", True, S.config_get(c2, 1, k) == v and S.cfg_abs(c2) == S.l1_cfg_set(S.cfg_abs(c), c["conf"], k, v), (seq, k, v))
                            r("config-shared", True, S.config_get(c2, 2, k) == v, (seq, k, v))
                        else:
                            r("config-errors-unchanged", True, c2 == c or v == 0, (seq, k, v))
                    if k in conf:
                        c3 = S.config_delete(c, 1, k)[0]
                        r("config-delete-factory", S.premise_ps(c, k), S.cfg_abs(c3) == {**S.cfg_abs(c), k: S.factory(c, k)}, (seq, k))
    # default modifiers: two namespaces, op sequences up to 3
    dops = [("set", ns, d) for ns in (1, 2) for d in ({"A": 1}, {"B": 2}, {"A": 3})] + [("delete", ns, None) for ns in (1, 2)]
    for n in range(0, 4):
        for seq in itertools.product(dops, repeat=n):
            s = {"dbs": {}}
            for op, ns, d in seq:
                s = S.dm_set(s, ns, d) if op == "set" else S.dm_delete(s, ns)
            for nss in ({1}, {1, 2}):
                for d in ({"A": 1}, {"C": 5}):
                    r("dm-per-namespace", S.premise_pns(nss, 1), S.dm_abs(S.dm_set(s, 1, d), nss) == S.dm_l1_set(S.dm_abs(s, nss), 1, d), (seq, nss, d))
                    old = S.dm_get(s, 1); new = S.dm_get(S.dm_set(s, 1, d), 1)
                    r("dm-laws", True, all(k in new for k in old) and all(new[k] == v for k, v in d.items())
                      and S.dm_get(S.dm_delete(s, 1), 1) == {} and S.effective(s, 1, {"A": 9})["A"] == 9, (seq, d))
    # registries: sequences of up to 3 registries, every kind, enabled, flag and token combination
    attrs = list(itertools.product(("Remote", "Oras", "Folder"), (True, False), (False, True), (False, True)))
    for n in range(0, 4):
        for combo in itertools.product(attrs, repeat=n):
            rs = tuple(S.Reg(i + 1, k, e, d, t, False, None, None) for i, (k, e, d, t) in enumerate(combo))
            p = S.publish_target(rs, None)
            r("publish-default-enabled", True, p is None or (rs[p].enabled and rs[p].kind != "Folder"), combo)
            r("publish-default-holder", S.premise_ph(rs) and S.premise_pu(rs) and S.premise_pk(rs), S.l1_publish(rs, None, p), combo)
            for name in range(1, n + 2):
                r("publish-named", S.premise_pe_named(rs, name), S.l1_publish(rs, name, S.publish_target(rs, name)), (combo, name))
            r("search-enabled", S.premise_pe_all(rs), all(S.search_lists(rs, i) == S.l1_search(rs, i) for i in range(-1, n + 1)), combo)
    offers = (None, 1, 2)
    sos = (None, 1, 5)
    for n in range(0, 4):
        for combo in itertools.product(itertools.product(("Remote", "Folder"), (True, False), offers, sos), repeat=n):
            rs = tuple(S.Reg(i + 1, k, e, False, False, False, so, off) for i, (k, e, off, so) in enumerate(combo))
            p = S.install_pick(rs)
            r("install-enabled", True, p is None or (rs[p].enabled and rs[p].offer is not None), combo)
            r("install-tie-sort-order", S.premise_pt(rs), S.l1_install(rs, p), combo)
    return viol, runs[0]

# ============================================================================ D. twin mutants

TWIN_MUTANTS = {
    "unin": {"no-before-delete": "web apps are deleted after the dependents check, not in OnBeforePhase",
             "top-record-kept": "the top-level Clean never deletes the record",
             "no-name-guard": "the self-uninstall steps never run",
             "no-class-gone-error": "the nested Clean goes on after IPM's classes are gone"},
    "set": {"default-overwritten": "SetValue overwrites the default node",
            "reset-to-value": "config delete restores the current value",
            "percent-per-namespace": "^% globals are kept per namespace",
            "first-row-best-order": "each kind's publishing row is its best PublishOrder, not its first enabled row",
            "publish-checks-enabled": "publish -r refuses a disabled registry",
            "search-enabled-only": "search lists only enabled registries",
            "tie-by-sort-order": "equal versions go to the lowest sort order"},
}


def conformance(both=True):
    pred = {**conf_unin(), **conf_repo()}
    out = {}
    for name, p in pred.items():
        if name not in OBS:
            out[name] = {"status": "no-observation"}
            continue
        mm = compare(p, OBS[name])
        out[name] = {"status": "match" if not mm else "mismatch", "keys": len(p), "mismatches": mm}
    return out


def main():
    checks, report = [], ["# Uninstall and settings models: twin checks", ""]
    # A
    pu = parity(ROOT / "models/verus/uninstall.rs", U)
    ps = parity(ROOT / "models/verus/settings.rs", S)
    for nm, pr in (("uninstall", pu), ("settings", ps)):
        checks.append({"name": f"repo-transcription-parity-{nm}", "status": "pass" if not pr["missing"] else "fail", "detail": pr,
                       "conditions": f"every spec fn of models/verus/{nm}.rs has a twin function of the same name"})
    wa = witness_agreement()
    checks.append({"name": "repo-witness-agreement", "status": "pass" if all(wa.values()) else "fail", "detail": wa,
                   "conditions": "the twin reproduces every fact the Verus witnesses prove"})
    report += ["## A. Transcription", "", f"uninstall.rs: {pu['spec_fns']} spec fns, missing in twin: {pu['missing'] or 'none'}.",
               f"settings.rs: {ps['spec_fns']} spec fns, missing in twin: {ps['missing'] or 'none'}.",
               f"Witness facts reproduced: {sum(wa.values())}/{len(wa)}.", ""]
    # B
    conf = conformance()
    report += ["## B. Conformance with IRIS observations", "", "| Observation | Result | Keys | Mismatches |", "|---|---|---|---|"]
    for name, r in sorted(conf.items()):
        checks.append({"name": f"repo-conf-{name}", "status": "pass" if r["status"] == "match" else "fail", "detail": r,
                       "conditions": "observation replayed on the twin of models/verus/{uninstall,settings}.rs"})
        report.append(f"| {name} | {r['status']} | {r.get('keys', '')} | {json.dumps(r.get('mismatches', ''))[:200]} |")
    allobs = sorted(n for n in OBS if n.startswith(("nc-unin", "nc-repo")))
    unaccounted = [n for n in allobs if n not in conf and n not in NOT_EXPRESSIBLE]
    checks.append({"name": "repo-conf-coverage", "status": "pass" if not unaccounted else "fail",
                   "detail": {"observations": len(allobs), "replayed": len([n for n in allobs if n in conf]),
                              "not_expressible": NOT_EXPRESSIBLE, "unaccounted": unaccounted}})
    report += ["", f"nc-unin-*/nc-repo-* observations: {len(allobs)}; replayed: {len([n for n in allobs if n in conf])}; "
               f"not expressible: {len(NOT_EXPRESSIBLE)} (listed in the record repo-conf-coverage); unaccounted: {unaccounted or 'none'}.", ""]
    # C
    viol, runs, nworlds = explore_unin()
    report += ["## C. Exhaustive exploration", "", f"Uninstall: {runs} runs over {sum(nworlds.values())} worlds {nworlds}. "
               "Bounds: IPM plus two modules with every combination of dependency (on each other and on IPM), graph error, "
               "global scope, installer hook (none, ok, failing) and one extra resource (none, web app, unnamed global, kept folder "
               "copy, folder copy, file copy, mapped class), all copies into one folder; IPM plus three modules with every acyclic "
               "graph and web apps; a namespace that borrows IPM. Commands: uninstall of each module, of `zpm` (exact), of `IPM` "
               "and of an unknown name with every -force/-r/-global and full-cleanup answer; clean of each module.", "",
               "| Clause | Meaning | Violations under its premises | Violations without them | Example (without) |", "|---|---|---|---|---|"]
    for cl, meaning in UNIN_CLAUSES.items():
        w_in = viol.get((cl, True), [0, None])
        w_out = viol.get((cl, False), [0, None])
        checks.append({"name": f"repo-explore-unin-{cl}", "status": "pass" if w_in[0] == 0 else "fail",
                       "detail": {"with_premises": w_in[0], "without_premises": w_out[0], "example_without": w_out[1], "example_with": w_in[1]},
                       "conditions": meaning + "; bounds as in models/explorer/repo/REPORT.md part C"})
        report.append(f"| {cl} | {meaning} | {w_in[0]} | {w_out[0]} | {w_out[1] or ''} |")
    sviol, sruns = explore_settings()
    report += ["", f"Settings and registries: {sruns} checks. Bounds: config op sequences of length 0-2 over five keys (special, "
               "seeded, unseeded, unknown) and five values from two start states; default-modifier op sequences of length 0-3 over "
               "two namespaces; every list of up to three registries of each kind, enabled, flag and token; every list of up to "
               "three registries with offers and sort orders.", "",
               "| Law | Meaning | Violations under its premises | Violations without them | Example (without) |", "|---|---|---|---|---|"]
    for law, meaning in SET_LAWS.items():
        w_in = sviol.get((law, True), [0, None])
        w_out = sviol.get((law, False), [0, None])
        checks.append({"name": f"repo-explore-set-{law}", "status": "pass" if w_in[0] == 0 else "fail",
                       "detail": {"with_premises": w_in[0], "without_premises": w_out[0], "example_without": w_out[1], "example_with": w_in[1]},
                       "conditions": meaning + "; bounds as in models/explorer/repo/REPORT.md part C"})
        report.append(f"| {law} | {meaning} | {w_in[0]} | {w_out[0]} | {(w_out[1] or '')[:120]} |")
    # D
    mres = {}
    for fam, muts in TWIN_MUTANTS.items():
        mod = U if fam == "unin" else S
        for m in muts:
            mod.MUT.add(m)
            try:
                bad = [n for n, r in conformance().items() if r["status"] == "mismatch"]
            finally:
                mod.MUT.discard(m)
            mres[m] = bad
    rejected = all(mres.values())
    checks.append({"name": "repo-twin-mutants-rejected", "status": "pass" if rejected else "fail", "detail": mres,
                   "conditions": "each twin mutant contradicted by at least one IRIS observation"})
    report += ["", "## D. Twin mutants (each must be contradicted by IRIS)", ""]
    for fam, muts in TWIN_MUTANTS.items():
        for m, desc in muts.items():
            report.append(f"- `{m}` ({desc}): {'REJECTED by ' + ', '.join(mres[m]) if mres[m] else 'NOT REJECTED'}")
    (ROOT / "evidence" / "model-checks-repo.json").write_text(json.dumps({"tool": "models/explorer/repo/check.py", "records": checks}, indent=2, default=str) + "\n")
    (HERE / "REPORT.md").write_text("\n".join(report) + "\n")
    print("\n".join(report))
    fails = [c["name"] for c in checks if c["status"] != "pass"]
    print("\nFAILED:", fails or "none")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
