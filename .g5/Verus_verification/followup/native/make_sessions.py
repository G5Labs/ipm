#!/usr/bin/env python3
"""Writes the follow-up native session scripts (sessions/*.cos) and their expectations
(cases/expected-*.json). Run with python3 -I -B make_sessions.py before any run; the runner
snapshots cases/ at the start of each run, so the expectations provably precede the observations.

Every expectation names its source: a model prediction (followup/models/*.py) or a source line.
"""
import json, os

HERE = os.path.dirname(os.path.abspath(__file__))
FX = "/home/irisowner/zpm/.followup/fixtures/r05"
PRE = ['set sc = $system.OBJ.LoadDir("/home/irisowner/zpm/.followup/scripts/FUNative/","ck-d",.err,1) write !,"HELPERS-LOADED ",$system.Status.IsOK(sc),!',
       'set sc = ##class(%IPM.Main).Shell("repo -n registry -disable") write !,"REGISTRY-DISABLED ",$system.Status.IsOK(sc),!']

# ---------------------------------------------------------------- R05: per-activation modes, nesting, installed state

R05 = [
    {"id": "N-R05-Q23F", "setup": [], "cmd": f"load {FX}/q23f/m/2.0.0/", "mods": ["fu-q23f-m", "fu-q23f-d"],
     "source": "loadtx prediction N-R05-Q23F; Utils/Module.cls:1172,1194-1196 (the dependency skips only its own tstart), :1357-1362",
     "expect": {"cmd.ok": "0", "after.fu-q23f-m.present": "0", "after.fu-q23f-d.present": "0", "after.fu-q23f-d.class": "0",
                "rows": "1", "row1.package": "fu-q23f-m", "row1.success": "ERR", "row1.committed": "0"}},
    {"id": "N-R05-Q23S", "setup": [], "cmd": f"load {FX}/q23s/m/2.0.0/", "mods": ["fu-q23s-m", "fu-q23s-d"],
     "source": "loadtx prediction N-R05-Q23S",
     "expect": {"cmd.ok": "1", "after.fu-q23s-m.present": "1", "after.fu-q23s-d.present": "1", "after.fu-q23s-d.class": "1",
                "rows": "2", "row1.package": "fu-q23s-m", "row2.package": "fu-q23s-d", "row2.committed": "1", "row2.success": "OK", "row1.committed": "1"}},
    {"id": "N-R05-NESTF", "setup": [], "cmd": f"load {FX}/nestf/m/2.0.0/", "mods": ["fu-nestf-m", "fu-nestf-d1", "fu-nestf-d2"],
     "source": "reduction in loadtx.py docstring: the top's LoadDependencies loads d2 then d1 with the top's params (Utils/Module.cls:1368-1405, Storage/Module.cls:1139-1154); d1's own NoJournal escapes the rollback (archived M5), d2 does not inherit it",
     "expect": {"cmd.ok": "0", "after.fu-nestf-m.present": "0", "after.fu-nestf-d2.present": "0", "after.fu-nestf-d2.class": "0",
                "after.fu-nestf-d1.present": "1", "after.fu-nestf-d1.class": "1", "rows": "1", "row1.package": "fu-nestf-m"}},
    {"id": "N-R05-NESTS", "setup": [], "cmd": f"load {FX}/nests/m/2.0.0/", "mods": ["fu-nests-m", "fu-nests-d1", "fu-nests-d2"],
     "source": "same reduction: rows in ID order top, d2, d1; d2 loaded once",
     "expect": {"cmd.ok": "1", "rows": "3", "row1.package": "fu-nests-m", "row2.package": "fu-nests-d2", "row3.package": "fu-nests-d1",
                "row2.committed": "1", "row3.committed": "1", "after.fu-nests-d2.present": "1", "after.fu-nests-d1.present": "1"}},
    {"id": "N-R05-DIA", "setup": [], "cmd": f"load {FX}/dia/m/2.0.0/", "mods": ["fu-dia-m", "fu-dia-d1", "fu-dia-d2", "fu-dia-s"],
     "source": "shared dependency loaded once, before its dependents (Utils/Module.cls:1381-1395)",
     "expect": {"cmd.ok": "1", "rows": "4", "row1.package": "fu-dia-m", "row2.package": "fu-dia-s",
                "after.fu-dia-s.present": "1", "after.fu-dia-d1.present": "1", "after.fu-dia-d2.present": "1", "after.fu-dia-m.present": "1"}},
    {"id": "N-R05-DEVUP", "setup": [f"load -dev {FX}/devup/d/1.0.0/"], "cmd": f"load {FX}/devup/m/2.0.0/", "mods": ["fu-devup-m", "fu-devup-d"],
     "source": "Storage/Module.cls:1139-1163 (installed 1.0.0 does not satisfy 2.0.0); Utils/Module.cls:307-311 (DeveloperMode overridden to 0 for a dependency); the developer-mode guard at :109 needs params(\"Install\"), which load never sets (Main.cls:2457-2460)",
     "expect": {"setup1.ok": "1", "before.fu-devup-d.dev": "1", "cmd.ok": "1", "after.fu-devup-d.version": "2.0.0", "after.fu-devup-d.dev": "0",
                "rows": "2", "row2.package": "fu-devup-d"}},
    {"id": "N-R05-DEVSAME", "setup": [f"load -dev {FX}/devsame/d/1.0.0/"], "cmd": f"load {FX}/devsame/m/2.0.0/", "mods": ["fu-devsame-m", "fu-devsame-d"],
     "source": "Storage/Module.cls:1139-1154: an installed version that satisfies the requirement is skipped whatever its developer mode",
     "expect": {"setup1.ok": "1", "cmd.ok": "1", "after.fu-devsame-d.version": "1.0.0", "after.fu-devsame-d.dev": "1", "rows": "1", "row1.package": "fu-devsame-m"}},
    {"id": "N-R05-HIGHER", "setup": [f"load {FX}/higher/d/3.0.0/"], "cmd": f"load {FX}/higher/m/2.0.0/", "mods": ["fu-higher-m", "fu-higher-d"],
     "source": "Storage/Module.cls:1157-1163 (no downgrade: >=3.0.0 and 1.0.0 cannot both hold) and :1180-1182 (Could not find satisfactory version)",
     "expect": {"setup1.ok": "1", "cmd.ok": "0", "after.fu-higher-m.present": "0", "after.fu-higher-d.version": "3.0.0", "rows": "1",
                "row1.success": "ERR", "row1.committed": "0"}},
    {"id": "N-R05-HIGHEROK", "setup": [f"load {FX}/higherok/d/3.0.0/"], "cmd": f"load {FX}/higherok/m/2.0.0/", "mods": ["fu-higherok-m", "fu-higherok-d"],
     "source": "Storage/Module.cls:1139-1154: 3.0.0 satisfies >=1.0.0 and is skipped",
     "expect": {"setup1.ok": "1", "cmd.ok": "1", "after.fu-higherok-m.present": "1", "after.fu-higherok-d.version": "3.0.0", "rows": "1"}},
    {"id": "N-R05-PARDEF", "setup": [], "cmd": f"load {FX}/pardef/m/2.0.0/", "mods": ["fu-pardef-m", "fu-pardef-d"],
     "source": "loadtx: the parent's <Defaults> are in the params its dependencies receive (Utils/Module.cls:1165-1169, :1323); without transactions everything written stays and both records read Committed=1 (:1224-1227)",
     "expect": {"cmd.ok": "0", "after.fu-pardef-m.present": "1", "after.fu-pardef-d.present": "1", "after.fu-pardef-d.class": "1", "rows": "2",
                "row1.package": "fu-pardef-m", "row1.committed": "1", "row1.success": "ERR", "row2.package": "fu-pardef-d", "row2.committed": "1", "row2.success": "OK"}},
]


def r05_session():
    lines = list(PRE)
    for c in R05:
        cid = c["id"]
        mods = ",".join(c["mods"])
        lines.append(f'do ##class(FUNative.Obs).Begin("{cid}")')
        for i, s in enumerate(c["setup"], 1):
            lines.append(f'do ##class(FUNative.Obs).Run("{cid}","setup{i}","{s}")')
        lines.append(f'do ##class(FUNative.Obs).Mods("{cid}","before.","{mods}") set base = ##class(FUNative.Obs).MaxHistId() hang 1.1')
        lines.append(f'do ##class(FUNative.Obs).Run("{cid}","cmd","{c["cmd"]}")')
        lines.append(f'do ##class(FUNative.Obs).Mods("{cid}","after.","{mods}") do ##class(FUNative.Obs).Rows("{cid}",base)')
        lines.append(f'do ##class(FUNative.Obs).End("{cid}")')
    lines += ['set sc = ##class(%IPM.Main).Shell("repo -n registry -enable")', 'write !,"NATIVE-END",!', "halt", ""]
    return "\n".join(lines)


# ---------------------------------------------------------------- N-P21: rollback of a journaled update followed by an unjournaled one

P21 = [
    {"id": "N-P21A", "source": "R06e question; N-P20(c) showed the journaled update alone is undone",
     "expect": {"value": "A", "tlevel": "0", "error": ""},
     "note": "expected under before-image rollback: the later unjournaled C is lost with the journaled B"},
    {"id": "N-P21AN", "source": "neighbor: N-P20(c) reproduced (unjournaled A, journaled B, rollback)", "expect": {"value": "A", "tlevel": "0", "error": ""}},
    {"id": "N-P21B", "source": "neighbor: the same writes, committed", "expect": {"value": "C", "tlevel": "0", "error": ""}},
    {"id": "N-P21R", "source": "the repair-103 dependency row through %IPM.General.History: unjournaled insert, journaled Committed=1, unjournaled Finalized=1, rollback",
     "expect": {"exists": "1", "committed": "0", "finalized": "0", "tlevel": "0", "error": ""}},
]


def p21_session():
    jm = "set jm = ##class(%IPM.Utils.JournalManager).%New(0)"
    L = list(PRE[:1])
    def tryrb(cid):
        return f'set err = "" try {{ trollback }} catch ex {{ set err = ex.DisplayString() }} do ##class(FUNative.Obs).Obs("{cid}","error",err) do ##class(FUNative.Obs).Obs("{cid}","tlevel",$tlevel)'
    L += ['do ##class(FUNative.Obs).Begin("N-P21A")', 'kill ^FUP21 tstart',
          f'{jm} set ^FUP21("a") = "A" kill jm', 'set ^FUP21("a") = "B"', f'{jm} set ^FUP21("a") = "C" kill jm',
          r'do ##class(FUNative.Obs).Obs("N-P21A","journaling.on",$select($zboolean($zjob,1024,1)=1024:0,1:1))', tryrb("N-P21A"),
          'do ##class(FUNative.Obs).Obs("N-P21A","value",$get(^FUP21("a"),"<undefined>")) kill ^FUP21', 'do ##class(FUNative.Obs).End("N-P21A")']
    L += ['do ##class(FUNative.Obs).Begin("N-P21AN")', 'kill ^FUP21 tstart', f'{jm} set ^FUP21("a") = "A" kill jm', 'set ^FUP21("a") = "B"', tryrb("N-P21AN"),
          'do ##class(FUNative.Obs).Obs("N-P21AN","value",$get(^FUP21("a"),"<undefined>")) kill ^FUP21', 'do ##class(FUNative.Obs).End("N-P21AN")']
    L += ['do ##class(FUNative.Obs).Begin("N-P21B")', 'kill ^FUP21 tstart', f'{jm} set ^FUP21("a") = "A" kill jm', 'set ^FUP21("a") = "B"',
          f'{jm} set ^FUP21("a") = "C" kill jm', 'tcommit do ##class(FUNative.Obs).Obs("N-P21B","error","") do ##class(FUNative.Obs).Obs("N-P21B","tlevel",$tlevel)',
          'do ##class(FUNative.Obs).Obs("N-P21B","value",$get(^FUP21("a"),"<undefined>")) kill ^FUP21', 'do ##class(FUNative.Obs).End("N-P21B")']
    L += ['do ##class(FUNative.Obs).Begin("N-P21R")', 'tstart',
          f'{jm} set h = ##class(%IPM.General.History).%New() set h.Action = "load", h.Package = "fu-p21-probe", h.CommandString = "followup probe", h.Version = ##class(%IPM.General.SemanticVersion).FromString("1.0.0") do h.%Save() set id = h.%Id() kill h, jm',
          'set h = ##class(%IPM.General.History).%OpenId(id) set h.Committed = 1 do h.%Save() kill h',
          f'{jm} set h = ##class(%IPM.General.History).%OpenId(id) set h.Finalized = 1, h.TimeEnd = $zdatetime($horolog,3) do h.%Save() kill h, jm',
          tryrb("N-P21R"),
          'set h = ##class(%IPM.General.History).%OpenId(id) do ##class(FUNative.Obs).Obs("N-P21R","exists",$isobject(h)) if $isobject(h) { do ##class(FUNative.Obs).Obs("N-P21R","committed",+h.Committed) do ##class(FUNative.Obs).Obs("N-P21R","finalized",+h.Finalized) } kill h',
          r'do ##class(%IPM.General.History).%DeleteId(id) do ##class(FUNative.Obs).Obs("N-P21R","probe.deleted",$select(##class(%IPM.General.History).%ExistsId(id):0,1:1))',
          'do ##class(FUNative.Obs).End("N-P21R")']
    L += ['write !,"NATIVE-END",!', "halt", ""]
    return "\n".join(L)


# ---------------------------------------------------------------- R04: history readers, narrowly scoped and read-only except one exact delete

R04 = [
    {"id": "N-R04-FIND", "source": "history_readers.py contract and as-built agree (valid neighbor): Package is a plain column",
     "expect": {"q.n": "2", "q.err": ""}},
    {"id": "N-R04-SEL", "source": "history_readers.py: as-built stops at Version_Major (collation puts it before lowercase package) and returns every row; the wrong variant M-SKIP-INVALID would return the 2 fu-hr-b rows; the contract wants 1",
     "expect": {"q.n": "@ALL", "q.err": ""}},
    {"id": "N-R04-SEL2", "source": "history_readers.py: Package sorts before Version_Major, so the Package conjunct is kept and the version is dropped (contract: 1 row)",
     "expect": {"q.n": "2", "q.err": ""}},
    {"id": "N-R04-ORDER", "source": "GetHistory ORDER BY ID ASC with TOP (History.cls:132-147)", "expect": {"q.n": "2", "q.rows": "@FIRST2"}},
    {"id": "N-R04-UNKNOWN", "source": "history_readers.py as-built: an unknown column yields no rows and no exception through GetHistory (native H2 saw the same through find)",
     "expect": {"q.n": "0"}},
    {"id": "N-R04-DEL", "source": "history_readers.py valid neighbor: delete with a plain-column filter removes exactly the selection",
     "expect": {"cmd.ok": "1", "del.before": "2", "del.after": "0", "others.unchanged": "1", "total.delta": "2"}},
    {"id": "N-R04-CUTOFF", "source": "History.cls:162-181: the purge compares TimeStart with the quoted cutoff; history_readers.py as-built predicts 0 selected rows where the contract selects the 40-day-old row",
     "expect": {"q.n": "0", "truth.n": "1"}},
]


def r04_session():
    L = list(PRE[:1])
    L += ['set ids = "" for spec = "fu-hr-a,1.0.0,40", "fu-hr-a,2.0.0,1", "fu-hr-b,1.0.0,1", "fu-hr-b,2.0.0,1", "fu-hr-del,1.0.0,1", "fu-hr-del,2.0.0,1" '
          '{ set ids = ids_$listbuild(##class(FUNative.Obs).Seed($piece(spec,",",1),$piece(spec,",",2),$piece(spec,",",3))) } write !,"SEEDED ",$listtostring(ids),!',
          'set total = ##class(FUNative.Obs).CountAll()']
    L += ['do ##class(FUNative.Obs).Begin("N-R04-FIND") do ##class(FUNative.Obs).Obs("N-R04-FIND","total",total) do ##class(FUNative.Obs).Find("N-R04-FIND","q","Package=fu-hr-a") do ##class(FUNative.Obs).End("N-R04-FIND")']
    L += ['do ##class(FUNative.Obs).Begin("N-R04-SEL") do ##class(FUNative.Obs).Obs("N-R04-SEL","total",total) do ##class(FUNative.Obs).Find("N-R04-SEL","q","Version_Major=1;package=fu-hr-b") do ##class(FUNative.Obs).End("N-R04-SEL")']
    L += ['do ##class(FUNative.Obs).Begin("N-R04-SEL2") do ##class(FUNative.Obs).Obs("N-R04-SEL2","total",total) do ##class(FUNative.Obs).Find("N-R04-SEL2","q","Package=fu-hr-b;Version_Major=1") do ##class(FUNative.Obs).End("N-R04-SEL2")']
    L += ['do ##class(FUNative.Obs).Begin("N-R04-ORDER") do ##class(FUNative.Obs).Obs("N-R04-ORDER","first2",$listtostring($list(ids,1,1))_","_$listtostring($list(ids,2,2))) do ##class(FUNative.Obs).Find("N-R04-ORDER","q","Package=fu-hr-*",1,2) do ##class(FUNative.Obs).End("N-R04-ORDER")']
    L += ['do ##class(FUNative.Obs).Begin("N-R04-UNKNOWN") do ##class(FUNative.Obs).Find("N-R04-UNKNOWN","q","NoSuchCol=1") do ##class(FUNative.Obs).End("N-R04-UNKNOWN")']
    L += ['do ##class(FUNative.Obs).Begin("N-R04-DEL") set before = ##class(FUNative.Obs).CountPrefix("fu-hr-del") set others = ##class(FUNative.Obs).CountAll() - before '
          'do ##class(FUNative.Obs).Obs("N-R04-DEL","del.before",before)',
          'do ##class(FUNative.Obs).Run("N-R04-DEL","cmd","history delete -DPackage=fu-hr-del")',
          'set after = ##class(FUNative.Obs).CountPrefix("fu-hr-del") do ##class(FUNative.Obs).Obs("N-R04-DEL","del.after",after) '
          'do ##class(FUNative.Obs).Obs("N-R04-DEL","others.unchanged",(##class(FUNative.Obs).CountAll() - after) = others) '
          'do ##class(FUNative.Obs).Obs("N-R04-DEL","total.delta",total - ##class(FUNative.Obs).CountAll()) do ##class(FUNative.Obs).End("N-R04-DEL")']
    L += ['do ##class(FUNative.Obs).Begin("N-R04-CUTOFF") set cutoff = $system.SQL.Functions.DATEADD("d", -30, +$horolog) do ##class(FUNative.Obs).Obs("N-R04-CUTOFF","cutoff",cutoff) '
          r"""do ##class(FUNative.Obs).Find("N-R04-CUTOFF","q","Package=fu-hr-a;TimeStart=<='"_cutoff_"'") """
          'do ##class(FUNative.Obs).Find("N-R04-CUTOFF","truth","Package=fu-hr-a;TimeStart=<="_cutoff) do ##class(FUNative.Obs).End("N-R04-CUTOFF")']
    L += ['for i=1:1:$listlength(ids) { do ##class(%IPM.General.History).%DeleteId($list(ids,i)) } write !,"UNSEEDED",!', 'write !,"NATIVE-END",!', "halt", ""]
    return "\n".join(L)


def main():
    os.makedirs(os.path.join(HERE, "sessions"), exist_ok=True)
    os.makedirs(os.path.join(HERE, "cases"), exist_ok=True)
    for name, text, exp in (("r05", r05_session(), R05), ("p21", p21_session(), P21), ("r04", r04_session(), R04)):
        open(os.path.join(HERE, "sessions", f"{name}.cos"), "w").write(text)
        json.dump({"cases": exp}, open(os.path.join(HERE, "cases", f"expected-{name}.json"), "w"), indent=1)
    print("sessions and expectations written")


if __name__ == "__main__":
    main()
