#!/usr/bin/env python3
"""Fixtures for the stage-C round-1 native cases (dispositions R1-ONT-04). Run with python3 -I make_stagec.py.
Writes native/fixtures/stagec/. Archives are built with the host tar (module.xml at the archive root)."""
import os, shutil, subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "fixtures", "stagec")

CLASS = """Class {pkg}.Main
{{

ClassMethod Hello() As %String
{{
    quit "hello from {name} {version}"
}}

}}
"""


def pkg_of(name):
    return "".join(p[:1].upper() + p[1:] for p in name.split("-"))


def module(d, name, version, extra_docs=""):
    pkg = pkg_of(name)
    os.makedirs(os.path.join(d, "src", pkg), exist_ok=True)
    with open(os.path.join(d, "src", pkg, "Main.cls"), "w") as fh:
        fh.write(CLASS.format(pkg=pkg, name=name, version=version))
    xml = ['<?xml version="1.0" encoding="UTF-8"?>', '<Export generator="IRIS" version="26">',
           f'  <Document name="{name}.ZPM">', "    <Module>", f"      <Name>{name}</Name>",
           f"      <Version>{version}</Version>", "      <Packaging>module</Packaging>",
           "      <SourcesRoot>src</SourcesRoot>", f'      <Resource Name="{pkg}.PKG"/>', "    </Module>",
           "  </Document>", extra_docs, "</Export>", ""]
    with open(os.path.join(d, "module.xml"), "w") as fh:
        fh.write("\n".join(x for x in xml if x != ""))
        fh.write("\n")


def class_doc(cls):
    return (f'  <Class name="{cls}">\n    <Super>%RegisteredObject</Super>\n'
            f'    <Method name="Hello">\n      <ClassMethod>1</ClassMethod>\n'
            f'      <Implementation><![CDATA[ quit "extra document" ]]></Implementation>\n    </Method>\n  </Class>')


def tarball(src_dir, out_path):
    subprocess.run(["tar", "czf", out_path, "-C", src_dir, "."], check=True)


def main():
    if os.path.isdir(ROOT):
        shutil.rmtree(ROOT)
    work = os.path.join(ROOT, "_work")
    # (a) .tar.gz and its .tgz neighbour (help Main.cls:280-282, CHANGELOG #474)
    for name, ext in (("s1-targz", "tar.gz"), ("s1-tgz", "tgz")):
        d = os.path.join(work, name)
        module(d, name, "1.0.0")
        os.makedirs(os.path.join(ROOT, "s1"), exist_ok=True)
        tarball(d, os.path.join(ROOT, "s1", f"{name}-1.0.0.{ext}"))
    # (b) late manifest checks: a module.xml with the .ZPM document plus a class document ("multiple documents"),
    # and one holding only a class document ("No module definition found")
    for name in ("s2-multi-notx", "s2-multi-def"):
        module(os.path.join(ROOT, "s2", name, "1.0.0"), name, "1.0.0", extra_docs=class_doc(pkg_of(name) + "Extra.Main"))
    d = os.path.join(ROOT, "s2", "s2-nomod-notx", "1.0.0")
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "module.xml"), "w") as fh:
        fh.write('<?xml version="1.0" encoding="UTF-8"?>\n<Export generator="IRIS" version="26">\n'
                 + class_doc("S2NomodNotxExtra.Main") + "\n</Export>\n")
    # (c) update -path with an archive of a different module (A2 OBS-L6), and a valid update neighbour
    for a in ("s3-a1", "s3-a2", "s3-a3"):
        module(os.path.join(ROOT, "s3", a, "1.0.0"), a, "1.0.0")
    os.makedirs(os.path.join(ROOT, "s3", "archives"), exist_ok=True)
    for name, ver in (("s3-b1", "1.0.0"), ("s3-b2", "1.0.0"), ("s3-a3", "2.0.0")):
        d = os.path.join(work, f"{name}-{ver}")
        module(d, name, ver)
        tarball(d, os.path.join(ROOT, "s3", "archives", f"{name}-{ver}.tgz"))
    # (d) an existing relative path, with absolute and nonexistent-relative neighbours
    module(os.path.join(ROOT, "s4", "s4-rel", "1.0.0"), "s4-rel", "1.0.0")
    module(os.path.join(ROOT, "s4", "s4-abs", "1.0.0"), "s4-abs", "1.0.0")
    shutil.rmtree(work)
    for dp, _, files in sorted(os.walk(ROOT)):
        for f in sorted(files):
            print(os.path.relpath(os.path.join(dp, f), ROOT))


if __name__ == "__main__":
    main()
