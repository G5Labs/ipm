#!/usr/bin/env python3
"""Writes the follow-up fixture modules under fixtures/r05/. Run with python3 -I -B make_fixtures.py.

Each module has one class <Pkg>.Main. A parent that must fail has an Invoke returning an error
(the mechanism of the original fixtures, e.g. native/fixtures/replay/r25b). Dependencies the parent
loads sit in the parent's .modules/ folder, which LoadModuleReference registers as a temporary
filesystem repository (Utils/Module.cls:134-139).
"""
import os, shutil

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "fixtures", "r05")


def pkg(name):
    return "".join(p[:1].upper() + p[1:] for p in name.split("-"))


def module(dirpath, name, version, deps=(), defaults=None, fail=False):
    os.makedirs(os.path.join(dirpath, "src", pkg(name)), exist_ok=True)
    x = ['<?xml version="1.0" encoding="UTF-8"?>', '<Export generator="IRIS" version="26">', f'  <Document name="{name}.ZPM">', "    <Module>",
         f"      <Name>{name}</Name>", f"      <Version>{version}</Version>", "      <Packaging>module</Packaging>", "      <SourcesRoot>src</SourcesRoot>",
         f'      <Resource Name="{pkg(name)}.PKG"/>']
    if deps:
        x.append("      <Dependencies>")
        for dn, dv in deps:
            x += ["        <ModuleReference>", f"          <Name>{dn}</Name>", f"          <Version>{dv}</Version>", "        </ModuleReference>"]
        x.append("      </Dependencies>")
    if defaults:
        x.append("      <Defaults>")
        x += [f'        <Parameter Name="{k}">{v}</Parameter>' for k, v in defaults.items()]
        x.append("      </Defaults>")
    if fail:
        x.append(f'      <Invoke Class="{pkg(name)}.Main" Method="Fail" CheckStatus="1"/>')
    x += ["    </Module>", "  </Document>", "</Export>", ""]
    open(os.path.join(dirpath, "module.xml"), "w").write("\n".join(x))
    open(os.path.join(dirpath, "src", pkg(name), "Main.cls"), "w").write(
        f"Class {pkg(name)}.Main\n{{\n\nClassMethod Hello() As %String\n{{\n    quit \"{name} {version}\"\n}}\n\n"
        f"ClassMethod Fail() As %Status\n{{\n    quit $$$ERROR($$$GeneralError, \"follow-up deliberate failure in {name}\")\n}}\n\n}}\n")


def main():
    shutil.rmtree(ROOT, ignore_errors=True)
    # Q-23: dependency whose own <Defaults> set only NoTransaction, parent fails / succeeds
    for case, fail in (("q23f", True), ("q23s", False)):
        m, d = f"fu-{case}-m", f"fu-{case}-d"
        base = os.path.join(ROOT, case, "m", "2.0.0")
        module(base, m, "2.0.0", deps=[(d, "1.0.0")], fail=fail)
        module(os.path.join(base, ".modules", d), d, "1.0.0", defaults={"NoTransaction": 1})
    # nested chain m -> d1 (own NoJournal) -> d2, parent fails / succeeds
    for case, fail in (("nestf", True), ("nests", False)):
        m, d1, d2 = f"fu-{case}-m", f"fu-{case}-d1", f"fu-{case}-d2"
        base = os.path.join(ROOT, case, "m", "2.0.0")
        module(base, m, "2.0.0", deps=[(d1, "1.0.0")], fail=fail)
        module(os.path.join(base, ".modules", d1), d1, "1.0.0", deps=[(d2, "1.0.0")], defaults={"NoJournal": 1})
        module(os.path.join(base, ".modules", d2), d2, "1.0.0")
    # diamond m -> d1 -> s, m -> d2 -> s
    m, d1, d2, s = "fu-dia-m", "fu-dia-d1", "fu-dia-d2", "fu-dia-s"
    base = os.path.join(ROOT, "dia", "m", "2.0.0")
    module(base, m, "2.0.0", deps=[(d1, "1.0.0"), (d2, "1.0.0")])
    module(os.path.join(base, ".modules", d1), d1, "1.0.0", deps=[(s, "1.0.0")])
    module(os.path.join(base, ".modules", d2), d2, "1.0.0", deps=[(s, "1.0.0")])
    module(os.path.join(base, ".modules", s), s, "1.0.0")
    # dependency installed in developer mode at another version / the same version
    for case, want in (("devup", "2.0.0"), ("devsame", "1.0.0")):
        m, d = f"fu-{case}-m", f"fu-{case}-d"
        module(os.path.join(ROOT, case, "d", "1.0.0"), d, "1.0.0")
        base = os.path.join(ROOT, case, "m", "2.0.0")
        module(base, m, "2.0.0", deps=[(d, want)])
        module(os.path.join(base, ".modules", d), d, want)
    # dependency installed at a higher version than required: exact requirement / range
    for case, req in (("higher", "1.0.0"), ("higherok", ">=1.0.0")):
        m, d = f"fu-{case}-m", f"fu-{case}-d"
        module(os.path.join(ROOT, case, "d", "3.0.0"), d, "3.0.0")
        base = os.path.join(ROOT, case, "m", "2.0.0")
        module(base, m, "2.0.0", deps=[(d, req)])
        module(os.path.join(base, ".modules", d), d, "1.0.0")
    # parent <Defaults> NoTransaction inherited by its dependency, parent fails
    m, d = "fu-pardef-m", "fu-pardef-d"
    base = os.path.join(ROOT, "pardef", "m", "2.0.0")
    module(base, m, "2.0.0", deps=[(d, "1.0.0")], defaults={"NoTransaction": 1}, fail=True)
    module(os.path.join(base, ".modules", d), d, "1.0.0")
    print("fixtures written under", ROOT)


if __name__ == "__main__":
    main()
