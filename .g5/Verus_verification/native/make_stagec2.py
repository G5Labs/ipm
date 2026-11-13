#!/usr/bin/env python3
"""Fixtures for round-1 model review case R1-MOD-05 (dependency with <Defaults> NoJournal under a normal-mode parent).
Writes native/fixtures/stagec2/. Run with python3 -I make_stagec2.py."""
import os, shutil

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "fixtures", "stagec2")
CLASS = """Class {pkg}.Main
{{

ClassMethod Hello() As %String
{{
    quit "hello from {name}"
}}

ClassMethod Fail() As %Status
{{
    quit $$$ERROR($$$GeneralError, "b2 deliberate failure in {name}")
}}

}}
"""


def pkg_of(name):
    return "".join(p[:1].upper() + p[1:] for p in name.split("-"))


def module(d, name, deps=(), fail=False, defaults=None, origin=""):
    pkg = pkg_of(name)
    os.makedirs(os.path.join(d, "src", pkg), exist_ok=True)
    open(os.path.join(d, "src", pkg, "Main.cls"), "w").write(CLASS.format(pkg=pkg, name=name))
    x = ['<?xml version="1.0" encoding="UTF-8"?>', '<Export generator="IRIS" version="26">']
    if origin:
        x.append(f"  <!-- {origin} -->")
    x += [f'  <Document name="{name}.ZPM">', "    <Module>", f"      <Name>{name}</Name>", "      <Version>1.0.0</Version>",
          "      <Packaging>module</Packaging>", "      <SourcesRoot>src</SourcesRoot>", f'      <Resource Name="{pkg}.PKG"/>']
    if defaults:
        x.append("      <Defaults>")
        for k, v in defaults.items():
            x.append(f'        <Parameter Name="{k}">{v}</Parameter>')
        x.append("      </Defaults>")
    if deps:
        x.append("      <Dependencies>")
        for dn in deps:
            x += ["        <ModuleReference>", f"          <Name>{dn}</Name>", "          <Version>1.0.0</Version>", "        </ModuleReference>"]
        x.append("      </Dependencies>")
    if fail:
        x.append(f'      <Invoke Class="{pkg}.Main" Method="Fail" CheckStatus="1"/>')
    x += ["    </Module>", "  </Document>", "</Export>", ""]
    open(os.path.join(d, "module.xml"), "w").write("\n".join(x))


def main():
    if os.path.isdir(ROOT):
        shutil.rmtree(ROOT)
    for p, d, dflt in (("m5-p", "m5-d", {"NoJournal": "1"}), ("m5n-p", "m5n-d", None)):
        pd = os.path.join(ROOT, p, "1.0.0")
        module(pd, p, deps=[d], fail=True, origin="R1-MOD-05: parent Activate fails after its dependency loaded")
        module(os.path.join(pd, ".modules", d), d, defaults=dflt,
               origin="Defaults syntax from the maintainer doc Utils/Module.cls:1033-1040" if dflt else "neighbour without Defaults")
    for dp, _, fs in sorted(os.walk(ROOT)):
        for f in sorted(fs):
            print(os.path.relpath(os.path.join(dp, f), ROOT))


if __name__ == "__main__":
    main()
