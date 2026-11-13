#!/usr/bin/env python3
"""Generate B2 fixture modules under native/fixtures/<name>/1.0.0/. Run with python3 -I make_fixtures.py."""
import os, shutil

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "fixtures")

def pkg_of(name):
    return "".join(p.capitalize() for p in name.split("-"))

CLASS = """Class {pkg}.Main
{{

ClassMethod Hello() As %String
{{
    quit "hello from {name}"
}}

/// Invoked in the Activate phase by fixtures that must fail there.
ClassMethod Fail() As %Status
{{
    quit $$$ERROR($$$GeneralError, "b2 deliberate failure in {name}")
}}

/// Invoked in the Activate phase by fixtures that must stall there.
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

def module(dirpath, name, deps=(), invoke=None, sysreq=None, origin=None):
    pkg = pkg_of(name)
    os.makedirs(os.path.join(dirpath, "src", pkg), exist_ok=True)
    with open(os.path.join(dirpath, "src", pkg, "Main.cls"), "w") as fh:
        fh.write(CLASS.format(pkg=pkg, name=name))
    lines = ['<?xml version="1.0" encoding="UTF-8"?>', '<Export generator="IRIS" version="26">']
    if origin:
        lines.append(f"  <!-- {origin} -->")
    lines += [f'  <Document name="{name}.ZPM">', "    <Module>", f"      <Name>{name}</Name>",
              "      <Version>1.0.0</Version>", "      <Packaging>module</Packaging>"]
    if sysreq:
        lines.append(f"      <SystemRequirements {sysreq}/>")
    lines += ["      <SourcesRoot>src</SourcesRoot>", f'      <Resource Name="{pkg}.PKG"/>']
    if deps:
        lines.append("      <Dependencies>")
        for d in deps:
            lines += ["        <ModuleReference>", f"          <Name>{d}</Name>", "          <Version>1.0.0</Version>",
                      "        </ModuleReference>"]
        lines.append("      </Dependencies>")
    if invoke:
        method, extra = (invoke, "") if isinstance(invoke, str) else invoke
        lines.append(f'      <Invoke Class="{pkg}.Main" Method="{method}"{extra} CheckStatus="1"/>')
    lines += ["    </Module>", "  </Document>", "</Export>", ""]
    with open(os.path.join(dirpath, "module.xml"), "w") as fh:
        fh.write("\n".join(lines))

def top(name, **kw):
    d = os.path.join(ROOT, name, "1.0.0")
    module(d, name, **kw)
    return d

def dep(parent_dir, name, **kw):
    module(os.path.join(parent_dir, ".modules", name), name, **kw)

def main():
    if os.path.isdir(ROOT):
        shutil.rmtree(ROOT)
    os.makedirs(ROOT)
    dep_origin = "dependency in .modules, layout adapted from tests/integration_tests/Test/PM/Integration/_data/simplest-module/3.0.0"
    inv_origin = "Invoke with CheckStatus adapted from tests/integration_tests/Test/PM/Integration/_data/invoke/failure/ReturnErrorCheckStatus"
    sys_origin = "SystemRequirements SYSNamespace adapted from tests/integration_tests/Test/PM/Integration/_data/sys-ns-module"
    # (a) dependency loads inside a parent load
    d = top("b2-par-ok", deps=["b2-dep-a0"], origin=dep_origin); dep(d, "b2-dep-a0")
    d = top("b2-par-actfail", deps=["b2-dep-a1"], invoke="Fail", origin=dep_origin + "; " + inv_origin); dep(d, "b2-dep-a1")
    d = top("b2-par-depfail", deps=["b2-dep-fail"], origin=dep_origin); dep(d, "b2-dep-fail", invoke="Fail", origin=inv_origin)
    d = top("b2-par-dev-ok", deps=["b2-dep-a2"], origin=dep_origin); dep(d, "b2-dep-a2")
    # (b) system requirement refusals
    top("b2-sysreq-ok", sysreq='Version="&gt;=2020.1"', origin="satisfiable SystemRequirements Version")
    for n in ("b2-sysreq-nd", "b2-sysreq-dev", "b2-sysreq-notx", "b2-sysreq-nojrn"):
        top(n, sysreq='SYSNamespace="true"', origin=sys_origin)
    # (c) a load that stalls in Activate
    for n in ("b2-hang-ok", "b2-hang-nd", "b2-hang-dev", "b2-hang-kill-nd", "b2-hang-kill-dev"):
        top(n, invoke="Wait", origin="Invoke that waits for a release flag; " + inv_origin)
    # Q-14: an Invoke bound to the Activate phase itself that returns an error
    top("b2-invoke-activate", invoke=("Fail", ' Phase="Activate" When="After"'), origin=inv_origin + " (Phase/When as in A2 OBS-H14)")
    # (d) caller-held transaction
    top("b2-ctx-commit"); top("b2-ctx-rb")

if __name__ == "__main__":
    main()
