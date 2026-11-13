#!/usr/bin/env python3
"""Bind B1 model cases (cases.json) to native fixtures and driver calls. Run with python3 -I make_replay.py.

Writes:
  native/cases/b1-cases-<sha8>.json      byte copy of cases.json (snapshotted by the runner with each run)
  native/cases/replay-binding.json       per-case realization: slots, setup, command, end mode, notes
  native/fixtures/replay/<prefix>/...    fixture modules (module m, dependencies d1/d2 in m's .modules)
  native/cos/replay-<group>.cos          terminal scripts: return, terminate, sigkill
cases.json is never modified.
"""
import hashlib, json, os, shutil, sys

HERE = os.path.dirname(os.path.abspath(__file__))
CASES = os.path.normpath(os.path.join(HERE, "..", "cases", "cases.json"))
# Accepted cases.json revisions: fc7adec4 (replayed in attempt b), 05842975 (B1 re-emission at 20:43Z: l1_permitted
# revised in 39 cases, C51 and C52 added; l2_predicted and inputs of the other cases unchanged).
PINS = {"fc7adec492c7a55f64ecddb44798359c652fbc21beef9973e01d531f123ec8f0",
        "058429752d6508a3e4527beb35e5825badf3132c8688b3f6138aa1d9bc457961"}
FIX = os.path.join(HERE, "fixtures", "replay")
# Binding attempt label, appended to every module-name prefix so a rerun starts from absent modules.
# Attempt 1 (no label, run 20261113T193745Z-replay-return) is kept with its recorded binding errors.
ATTEMPT = "b"
# C49T/K attempt b stalled after the C3 SetCommitted (binding later than the model's AfterC3 point);
# attempt c stalls on that SetCommitted line, after the C3 tcommit.
ATTEMPT_OVERRIDE = {"B1-C49T": "c", "B1-C49K": "c"}
CFIX = "/home/irisowner/zpm/.native/fixtures/replay"

CLASS = """Class {pkg}.Main
{{

ClassMethod Hello() As %String
{{
    quit "hello from {name} {version}"
}}

ClassMethod Fail() As %Status
{{
    quit $$$ERROR($$$GeneralError, "b2 deliberate failure in {name}")
}}

/// Signals ^IRIS.Temp.B2Signal(<package>,"pid") and waits up to 300 s for ...("release").
ClassMethod Wait() As %Status
{{
    set tag = $piece($classname(), ".", 1)
    set ^IRIS.Temp.B2Signal(tag, "tlevel") = $tlevel
    set ^IRIS.Temp.B2Signal(tag, "pid") = $job
    for i=1:1:1500 {{
        quit:$data(^IRIS.Temp.B2Signal(tag, "release"))
        hang 0.2
    }}
    quit $$$OK
}}

}}
"""

UPD = """Class {pkg}.Upd.V{major} Extends %IPM.General.Update.VersionBase
{{

ClassMethod GetOrderedMethodsPrimaryOnly() As %Library.DynamicArray
{{
    return []
}}

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{{
    return ["StepA"]
}}

ClassMethod StepA()
{{
    write !, "{pkg}.Upd.V{major}:StepA"
}}

}}
"""


def pkg_of(name):
    return "".join(p[:1].upper() + p[1:] for p in name.split("-"))


def module(d, name, version, deps=(), invoke=None, sysreq=False, update=False, extra_res=(), manifest="valid", origin=""):
    pkg = pkg_of(name)
    os.makedirs(os.path.join(d, "src", pkg), exist_ok=True)
    with open(os.path.join(d, "src", pkg, "Main.cls"), "w") as fh:
        fh.write(CLASS.format(pkg=pkg, name=name, version=version))
    if update:
        os.makedirs(os.path.join(d, "src", pkg, "Upd"), exist_ok=True)
        for major in range(1, int(version.split(".")[0]) + 1):
            with open(os.path.join(d, "src", pkg, "Upd", f"V{major}.cls"), "w") as fh:
                fh.write(UPD.format(pkg=pkg, major=major))
    head = ['<?xml version="1.0" encoding="UTF-8"?>', '<Export generator="IRIS" version="26">']
    if origin:
        head.append(f"  <!-- {origin} -->")
    if manifest == "no module document":
        lines = head + ["</Export>", ""]
    elif manifest.startswith("written, then refused (multiple documents)"):
        # the module's .ZPM document plus a class document in the same <Export>
        lines = head + [f'  <Document name="{name}.ZPM">', "    <Module>", f"      <Name>{name}</Name>",
                        f"      <Version>{version}</Version>", "      <Packaging>module</Packaging>",
                        "      <SourcesRoot>src</SourcesRoot>", f'      <Resource Name="{pkg}.PKG"/>', "    </Module>",
                        "  </Document>", f'  <Class name="{pkg}Extra.Main">', "    <Super>%RegisteredObject</Super>",
                        '    <Method name="Hello">', "      <ClassMethod>1</ClassMethod>",
                        '      <Implementation><![CDATA[ quit "extra document" ]]></Implementation>', "    </Method>",
                        "  </Class>", "</Export>", ""]
    else:
        lines = head + [f'  <Document name="{name}.ZPM">', "    <Module>", f"      <Name>{name}</Name>",
                        f"      <Version>{version}</Version>", "      <Packaging>module</Packaging>"]
        if manifest == "not parseable":
            # adapted from tests/integration_tests/Test/PM/Integration/_data/malformed-module (unclosed element)
            lines.append("      <Malformed>")
        if sysreq:
            lines.append('      <SystemRequirements SYSNamespace="true"/>')
        lines += ["      <SourcesRoot>src</SourcesRoot>", f'      <Resource Name="{pkg}.PKG"/>']
        for r in extra_res:
            lines.append(f'      <Resource Name="{r}"/>')
        if update:
            lines.append(f"      <UpdatePackage>{pkg}.Upd</UpdatePackage>")
        if deps:
            lines.append("      <Dependencies>")
            for dn, dv in deps:
                lines += ["        <ModuleReference>", f"          <Name>{dn}</Name>", f"          <Version>{dv}</Version>",
                          "        </ModuleReference>"]
            lines.append("      </Dependencies>")
        if invoke:
            method, attrs = invoke
            lines.append(f'      <Invoke Class="{pkg}.Main" Method="{method}"{attrs} CheckStatus="1"/>')
        lines += ["    </Module>", "  </Document>", "</Export>", ""]
    with open(os.path.join(d, "module.xml"), "w") as fh:
        fh.write("\n".join(lines))


def bind(case):
    cid = case["id"]
    i = case["inputs"]
    tp = i["top_package"]
    num = int(cid[4:6])
    suffix = cid[6:].lower()
    prefix = f"r{num:02d}{suffix}{ATTEMPT_OVERRIDE.get(cid, ATTEMPT)}"
    top = f"{prefix}-m"
    end_txt = i["end"]
    fault = i["fault_point"]
    b = {"id": cid, "prefix": prefix, "realized": "yes", "notes": [], "slots": {"top": top}, "setup": [],
         "extra": [], "waitTag": ""}
    approx_zb = "not natively injectable" in fault
    if approx_zb:
        b["realized"] = "approx"
        b["notes"].append("APPROXIMATION. No product hook runs user code between the final commit (C3) and Finalize. "
                          "The JOB sets a ZBREAK (action N with execute code) on the C3 block's `do pLog.SetCommitted()` in "
                          "LoadNewModule, after its tcommit (attempt c; attempt b used the later WriteAfterInstallMessage line), "
                          "which stalls the process; the harness then "
                          "ends it. The stall only changes timing, and a process end at that point is possible in production, "
                          "but the injection is a debugger facility, so this case is reported as approximate and never as agreeing.")
        b["extra"].append("zb-c3-setcommitted")
    # end mode
    if end_txt.startswith("process terminated"):
        endmode = "terminate"
    elif end_txt.startswith("process killed"):
        endmode = "sigkill"
    elif "caller rolls back" in end_txt:
        endmode = "return-rollback"
    else:
        endmode = "return"
    nest = "callertx" if "caller already in a transaction" in i["nesting"] else "top"
    b.update({"endmode": endmode, "nest": nest})
    tdir = os.path.join(FIX, prefix, "m")
    cdir = f"{CFIX}/{prefix}/m"
    # top invoke
    act = tp["activate"]
    invoke = None
    if act == "throws":
        invoke = ("Fail", "")
        b["notes"].append("Activate 'throws' realized by an Invoke (default Configure phase) returning an error with "
                          "CheckStatus=1; %Activate raises it, so ExecutePhases finalizes Activate with the error.")
    elif act.startswith("invoke returns error"):
        invoke = ("Fail", ' Phase="Activate" When="After"')
    if approx_zb:
        b["waitTag"] = pkg_of(top)
    if endmode in ("terminate", "sigkill") and "inside m's Activate" in fault:
        invoke = ("Wait", "")
        b["waitTag"] = pkg_of(top)
        b["notes"].append("Process end inside Activate realized by an Invoke that waits on an IRISTEMP flag; the "
                          "harness ends the JOB while it waits.")
    # dependencies
    deps = []
    for k, dspec in enumerate(i.get("dependencies", []), start=1):
        dname = f"{prefix}-d{k}"
        b["slots"][f"dep{k}"] = dname
        dver = "1.0.0+snapshot" if dspec.get("snapshot") else "1.0.0"
        deps.append((dname, dver, dspec))
        if dspec.get("snapshot"):
            b["notes"].append("Snapshot dependency realized as version 1.0.0+snapshot (IsSnapshot reads the build "
                              "label); its version is compared to the model's 1.0.0 after dropping +snapshot.")
    # top versions needed
    inst = i["installed_before"]["top"].strip()
    upd = tp.get("update_package", False)
    sysreq = not tp.get("system_requirements_met", True)
    manifest = tp.get("manifest", "valid")
    extra_res = []
    if manifest == "module document refused by validation":
        owner = f"{prefix}-own"
        shared = pkg_of(prefix) + "Shared.PKG"
        odir = os.path.join(FIX, prefix, "own", "1.0.0")
        module(odir, owner, "1.0.0", extra_res=(shared,), origin="owner of the shared resource")
        os.makedirs(os.path.join(odir, "src", pkg_of(prefix) + "Shared"), exist_ok=True)
        with open(os.path.join(odir, "src", pkg_of(prefix) + "Shared", "Main.cls"), "w") as fh:
            fh.write(CLASS.format(pkg=pkg_of(prefix) + "Shared", name=owner, version="1.0.0"))
        b["setup"].append(f"load {CFIX}/{prefix}/own/1.0.0/")
        extra_res = [shared]
        b["notes"].append(f"Validation refusal realized by declaring {shared}, already owned by setup module {owner} "
                          "(an extra module outside the model's slots).")
    if manifest == "not parseable":
        b["notes"].append("Unparseable manifest realized by an unclosed element, as in _data/malformed-module.")
    if manifest == "no module document":
        b["notes"].append("Manifest without a module document realized as an <Export> with no <Document>.")
    if manifest.startswith("written, then refused (multiple documents)"):
        b["notes"].append("Two-document manifest realized as the .ZPM document plus a class document <Pkg>Extra.Main.")
    module(os.path.join(tdir, "2.0.0"), top, "2.0.0", deps=[(dn, dv) for dn, dv, _ in deps], invoke=invoke,
           sysreq=sysreq, update=upd, extra_res=extra_res, manifest=manifest)
    for dn, dv, dspec in deps:
        dinv = None
        if dspec.get("activate") == "throws":
            dinv = ("Fail", "")
        if endmode in ("terminate", "sigkill") and "inside d's Activate" in fault and dn.endswith("-d1"):
            dinv = ("Wait", "")
            b["waitTag"] = pkg_of(dn)
        if dspec.get("found", True):
            module(os.path.join(tdir, "2.0.0", ".modules", dn), dn, dv, invoke=dinv)
        else:
            b["extra"].append("no-registry")
            b["notes"].append("Dependency 'not found' realized by omitting it from .modules and disabling the remote "
                              "'registry' repository for this case only (restored afterwards).")
        if dspec.get("installed_before", "absent").strip() != "absent":
            b["setup"].append(f"load {cdir}/2.0.0/.modules/{dn}/")
    if inst != "absent":
        ver = inst.split()[0]
        if ver != "2.0.0":
            module(os.path.join(tdir, ver), top, ver, update=upd)
        b["setup"].append(("load -dev " if "developer mode" in inst else "load ") + f"{cdir}/{ver}/")
    if "CleanUp" in fault:
        bdir = os.path.join(tdir, "2.0.0", ".modules", "broken")
        os.makedirs(bdir, exist_ok=True)
        with open(os.path.join(bdir, "module.xml"), "w") as fh:
            fh.write('<?xml version="1.0" encoding="UTF-8"?>\n<Export generator="IRIS" version="26">\n  <Document name="broken.ZPM">\n    <Module>\n      <Name>bad name</Name>\n      <Version>1.0.0</Version>\n      <Packaging>module</Packaging>\n    </Module>\n  </Document>\n</Export>\n')
        b["notes"].append("Error after the last commit realized as in A2 OBS-L12: .modules/broken/module.xml names a module "
                          "with a space, which fails %IPM.Repo.Filesystem.Cache validation, so BuildCache and "
                          "TempLocalRepoManager:%OnNew fail and CleanUp() runs on an invalid oref. (Attempt 1 used an "
                          "unparseable manifest, which BuildCache skips, and did not produce the fault.) "
                          "Realized only if the run reports that error.")
        b["fault_check"] = "INVALID OREF"
    # command
    flags = []
    if i["dev_flag"] in ("-dev", "-nodev"):
        flags.append(i["dev_flag"])
    if i.get("force"):
        flags.append("-force")
    mode = {"NoTransaction": " -DNoTransaction=1", "NoJournal": " -DNoJournal=1"}.get(i["mode"], "")
    path = f"{cdir}/2.0.0/"
    if fault.startswith("path refusal"):
        path = f"{prefix}-m/2.0.0"
        b["notes"].append("Relative, nonexistent path.")
    b["command"] = "load " + "".join(f + " " for f in flags) + path + mode
    b["extra"] = sorted(set(b["extra"]))
    return b


def cos_line(b):
    s = b["slots"]
    q = lambda v: '"' + v.replace('"', '""') + '"'
    args = [b["id"], s.get("top", ""), s.get("dep1", ""), s.get("dep2", ""), " ;; ".join(b["setup"]), b["command"],
            b["nest"], b["endmode"], b["waitTag"], ",".join(b["extra"])]
    return "do ##class(B2Native.Replay).Case(" + ",".join(q(a) for a in args) + ")"


def main():
    raw = open(CASES, "rb").read()
    sha = hashlib.sha256(raw).hexdigest()
    if sha not in PINS:
        print("cases.json hash changed:", sha, file=sys.stderr)
        sys.exit(1)
    copy = os.path.join(HERE, "cases", f"b1-cases-{sha[:8]}.json")
    with open(copy, "wb") as fh:
        fh.write(raw)
    if os.path.isdir(FIX):
        shutil.rmtree(FIX)
    cases = json.loads(raw)["cases"]
    bindings = [bind(c) for c in cases]
    json.dump({"cases_json": "cases/cases.json", "cases_sha256": sha, "copy": os.path.basename(copy),
               "bindings": bindings}, open(os.path.join(HERE, "cases", "replay-binding.json"), "w"), indent=1)
    head = ['set sc = $system.OBJ.LoadDir("/home/irisowner/zpm/.native/B2Native/","ck-d",.err,1) write !,"HELPERS-LOADED ",$system.Status.GetErrorText(sc),!']
    tail = ['write !,"NATIVE-END",!', "halt", ""]
    groups = {"return": [], "terminate": [], "sigkill": [], "approx": []}
    for b in bindings:
        if b["realized"] == "no":
            continue
        g = "approx" if b["realized"] == "approx" else ("return" if b["endmode"].startswith("return") else b["endmode"])
        groups[g].append(cos_line(b))
    only = [a for a in sys.argv[1:] if a.startswith("--only=")]
    if only:
        ids = only[0].split("=", 1)[1].split(",")
        lines = [cos_line(b) for b in bindings if b["id"] in ids and b["realized"] != "no"]
        with open(os.path.join(HERE, "cos", "replay-only.cos"), "w") as fh:
            fh.write("\n".join(head + lines + tail))
        print("replay-only.cos:", ids)
        return
    for g, lines in groups.items():
        with open(os.path.join(HERE, "cos", f"replay-{g}.cos"), "w") as fh:
            fh.write("\n".join(head + lines + tail))
    print(sha, {g: len(v) for g, v in groups.items()}, "not realized:", [b["id"] for b in bindings if b["realized"] == "no"])


if __name__ == "__main__":
    main()
