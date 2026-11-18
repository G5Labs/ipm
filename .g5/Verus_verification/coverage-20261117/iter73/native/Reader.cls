/// Test-only observer for iteration73. Not IPM code.
Class IPMCov.GenReader73
{

Parameter ROOT = "/tmp/iter73/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Read(pFile As %String) As %String
{
    quit:'##class(%File).Exists(pFile) ""
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(pFile)
    quit s.Read(1000000)
}

ClassMethod Text(pCase As %String) As %String
{
    quit ..Read(..#ROOT_pCase_"/module.xml")
}

/// Records whether module.xml of pCase contains pNeedle (or, with pCount, how many times).
ClassMethod Has(pKey As %String, pCase As %String, pNeedle As %String, pCount As %Boolean = 0)
{
    set t = ..Text(pCase)
    do ..Obs(pKey, $select(pCount:$length(t, pNeedle) - 1, 1:t[pNeedle))
}

/// Records each field of the single UnitTest element of pCase separately.
ClassMethod Unit(pCase As %String, pName As %String, pPackage As %String)
{
    set t = ..Text(pCase)
    set start = $find(t, "<UnitTest ")
    set el = $select(start:$extract(t, start - 10, $find(t, "/>", start) - 1), 1:"")
    do ..Obs(pCase_".unittest.tag", el'="")
    do ..Obs(pCase_".unittest.name", el[("Name="""_pName_""""))
    do ..Obs(pCase_".unittest.package", el[("Package="""_pPackage_""""))
    do ..Obs(pCase_".unittest.phase", el["Phase=""test""")
}

ClassMethod File(pKey As %String, pRel As %String)
{
    do ..Obs(pKey, ##class(%File).Exists(..#ROOT_pRel))
}

/// Counts module_*.xml files in pCase and whether one holds the fixture's old marker.
ClassMethod Backups(pCase As %String)
{
    set count = 0, old = 0
    set rs = ##class(%ResultSet).%New("%File:FileSet")
    do rs.Execute(..#ROOT_pCase_"/", "module_*.xml")
    while rs.Next() {
        set count = count + 1
        write !,"BACKUP|"_pCase_"|"_rs.Get("Name"),!
        set:..Read(rs.Get("Name"))["iter73-old-module-xml" old = 1
    }
    do ..Obs(pCase_".backup.count", count)
    do ..Obs(pCase_".backup.old", old)
    do ..Obs(pCase_".xml.old", ..Text(pCase)["iter73-old-module-xml")
}

ClassMethod Manifests()
{
    for m="m1","m3","m4","m2a","m2e","m2b","m5","m6","m7","m8","x1","x2","x3","x4" {
        write !,"XML|"_m_"|",$translate(..Text(m),$c(10,13),"  "),!
    }
    do ..Has("m1.Ia1.B.PKG@cls", "m1", "<Resource Directory=""cls"" Name=""Ia1.B.PKG""/>")
    do ..Has("m1.Ia1.PKG", "m1", "Name=""Ia1.PKG""")
    do ..Has("m1.Ia2.PKG@cls", "m1", "<Resource Directory=""cls"" Name=""Ia2.PKG""/>")
    do ..Has("m1.Ia2.B.PKG", "m1", "Name=""Ia2.B.PKG""")
    do ..Has("m1.Ia3.B.PKG@cls", "m1", "<Resource Directory=""cls"" Name=""Ia3.B.PKG""/>")
    do ..Has("m1.Ia3.PKG", "m1", "Name=""Ia3.PKG""")
    do ..Has("m1.Ia3.B.C.cls", "m1", "Name=""Ia3.B.C.")
    do ..Has("m1.Ia4.PKG@cls", "m1", "<Resource Directory=""cls"" Name=""Ia4.PKG""/>")
    do ..Has("m1.Ia5.PKG@cls", "m1", "<Resource Directory=""cls"" Name=""Ia5.PKG""/>")
    do ..Has("m1.Ir69.MAC@mac", "m1", "<Resource Directory=""mac"" Name=""Ir69.MAC""/>")
    do ..Has("m1.Ii69.INC@inc", "m1", "<Resource Directory=""inc"" Name=""Ii69.INC""/>")
    do ..Has("m1.notes", "m1", "notes")
    do ..Has("m1.Ij1", "m1", "Ij1")
    do ..Has("m1.unittest", "m1", "<UnitTest Name=""/tests/"" Package=""It1"" Phase=""test""/>")
    do ..Has("m1.unittest.count", "m1", "<UnitTest ", 1)
    do ..Unit("m1", "/tests/", "It1")
    do ..Has("m1.Ih69", "m1", "Ih69")
    do ..Has("m1.Inopkg69", "m1", "Inopkg69")
    do ..Has("m1.resource.count", "m1", "<Resource ", 1)
    do ..Has("m3.Id1.B.PKG@x", "m3", "<Resource Directory=""x"" Name=""Id1.B.PKG""/>")
    do ..Has("m3.Id1.B.PKG@y", "m3", "<Resource Directory=""y"" Name=""Id1.B.PKG""/>")
    do ..Has("m3.Id1.B.PKG.count", "m3", "Name=""Id1.B.PKG""", 1)
    do ..Has("m3.Id2.B.PKG@x", "m3", "<Resource Directory=""x"" Name=""Id2.B.PKG""/>")
    do ..Has("m3.Id2.D.PKG@y", "m3", "<Resource Directory=""y"" Name=""Id2.D.PKG""/>")
    do ..Has("m3.Id2.PKG", "m3", "Name=""Id2.PKG""")
    do ..Has("m3.unittest.count", "m3", "<UnitTest ", 1)
    do ..Has("m4.Ie1.PKG@cls", "m4", "<Resource Directory=""cls"" Name=""Ie1.PKG""/>")
    do ..Has("m4.unittest", "m4", "<UnitTest Name=""/src/tests/"" Package=""Ie1T"" Phase=""test""/>")
    do ..Unit("m4", "/src/tests/", "Ie1T")
    do ..Has("m4.Ie1T.resource", "m4", "Name=""Ie1T.")
    do ..Has("m4.Ihelp69", "m4", "Ihelp69")
    do ..Has("m4.Inopkg69b", "m4", "Inopkg69b")
    do ..Has("m4.resource.count", "m4", "<Resource ", 1)
    do ..Has("m2a.Ir2a.cls", "m2a", "<Resource Name=""Ir2a.cls""/>")
    do ..Has("m2a.Ib2a.P.PKG@cls", "m2a", "<Resource Directory=""cls"" Name=""Ib2a.P.PKG""/>")
    do ..Has("m2e.Ir2e.cls", "m2e", "<Resource Name=""Ir2e.cls""/>")
    do ..Has("m2b.Ik2b.P.PKG", "m2b", "<Resource Name=""Ik2b.P.PKG""/>")
    do ..Has("m5.Ig1.PKG@cls", "m5", "<Resource Directory=""cls"" Name=""Ig1.PKG""/>")
    do ..Has("m5.Ig2.PKG@cls", "m5", "<Resource Directory=""cls"" Name=""Ig2.PKG""/>")
    do ..Has("m5.Ig3", "m5", "Ig3")
    do ..Obs("m6.xml.exists", ##class(%File).Exists(..#ROOT_"m6/module.xml"))
    do ..Has("m7.Im7.PKG@a", "m7", "<Resource Directory=""a"" Name=""Im7.PKG""/>")
    do ..Has("m7.Im7.PKG@a/b", "m7", "<Resource Directory=""a/b"" Name=""Im7.PKG""/>")
    do ..Has("m7.Im7.PKG.count", "m7", "Name=""Im7.PKG""", 1)
    do ..Has("m8.Im8.PKG@a", "m8", "<Resource Directory=""a"" Name=""Im8.PKG""/>")
    do ..Has("m8.Im8.PKG@ab", "m8", "<Resource Directory=""ab"" Name=""Im8.PKG""/>")
    do ..Has("m8.Im8.PKG.count", "m8", "Name=""Im8.PKG""", 1)
    do ..Backups("m7")
    do ..Backups("m8")
    // generate -export: where the files went, and what module.xml says about them
    do ..File("x1.file.Ixa.P.Q", "x1/src/cls/Ixa/P/Q.cls")
    do ..File("x1.file.Ixa.R.Rtn.flat", "x1/src/mac/Ixa.R.Rtn.mac")
    do ..File("x1.file.Ixa.R.Rtn.nested", "x1/src/mac/Ixa/R/Rtn.mac")
    do ..Obs("x1.dir.int", ##class(%File).DirectoryExists(..#ROOT_"x1/src/int"))
    do ..Has("x1.Ixa.P.PKG@cls", "x1", "<Resource Directory=""cls"" Name=""Ixa.P.PKG""/>")
    do ..Has("x1.Ixa.R.Rtn.MAC", "x1", "<Resource Name=""Ixa.R.Rtn.MAC""/>")
    do ..Has("x1.resource.count", "x1", "<Resource ", 1)
    do ..File("x2.file.Ixb.P.Q", "x2/src/cls/Ixb/P/Q.cls")
    do ..File("x2.file.IxbRtn", "x2/src/mac/IxbRtn.mac")
    do ..Obs("x2.dir.int", ##class(%File).DirectoryExists(..#ROOT_"x2/src/int"))
    do ..Has("x2.Ixb.P.PKG@cls", "x2", "<Resource Directory=""cls"" Name=""Ixb.P.PKG""/>")
    do ..Has("x2.IxbRtn.MAC@mac", "x2", "<Resource Directory=""mac"" Name=""IxbRtn.MAC""/>")
    do ..Has("x2.resource.count", "x2", "<Resource ", 1)
    do ..File("x3.file.Ixc.P.Q", "x3/src/cls/Ixc/P/Q.cls")
    do ..File("x3.file.IxcInc", "x3/src/inc/IxcInc.inc")
    do ..Has("x3.Ixc.P.PKG@cls", "x3", "<Resource Directory=""cls"" Name=""Ixc.P.PKG""/>")
    do ..Has("x3.IxcInc.INC@inc", "x3", "<Resource Directory=""inc"" Name=""IxcInc.INC""/>")
    do ..Has("x3.resource.count", "x3", "<Resource ", 1)
    do ..File("x4.file.Ixd.P.Q", "x4/src/cls/Ixd/P/Q.cls")
    do ..File("x4.file.Ixd.I.Inc.flat", "x4/src/inc/Ixd.I.Inc.inc")
    do ..File("x4.file.Ixd.I.Inc.nested", "x4/src/inc/Ixd/I/Inc.inc")
    do ..Has("x4.Ixd.P.PKG@cls", "x4", "<Resource Directory=""cls"" Name=""Ixd.P.PKG""/>")
    do ..Has("x4.Ixd.I.Inc.INC", "x4", "<Resource Name=""Ixd.I.Inc.INC""/>")
    do ..Has("x4.resource.count", "x4", "<Resource ", 1)
}

ClassMethod Seeded(pPrefix As %String)
{
    for c="Ixa.P.Q","Ixb.P.Q","Ixc.P.Q","Ixd.P.Q" {
        do ..Obs(pPrefix_"."_c, ##class(%Dictionary.CompiledClass).%ExistsId(c))
    }
    for r="Ixa.R.Rtn.MAC","IxbRtn.MAC","IxcInc.INC","Ixd.I.Inc.INC" {
        do ..Obs(pPrefix_".rtn."_r, ##class(%Routine).Exists(r))
    }
}

/// Removes the exported items from the namespace so that `load` must bring them back from the files.
ClassMethod DeleteExported()
{
    for c="Ixa.P.Q","Ixb.P.Q","Ixc.P.Q","Ixd.P.Q" {
        do $system.OBJ.Delete(c, "-d")
    }
    for r="Ixa.R.Rtn.MAC","Ixa.R.Rtn.INT","IxbRtn.MAC","IxbRtn.INT","IxcInc.INC","Ixd.I.Inc.INC" {
        if ##class(%Routine).Exists(r) {
            set sc = ##class(%RoutineMgr).Delete(r)
            write !,"DELETE|"_r_"|"_sc,!
        }
    }
    do ..Seeded("pre")
}

ClassMethod Loaded()
{
    for m="m1","m3","m4","m2a","m2e","m2b","m7","m8","x1","x2","x3","x4" {
        do ..Obs(m_".load.ok", ##class(%IPM.Storage.Module).NameExists(m))
        set mod = ##class(%IPM.Storage.Module).NameOpen(m)
        continue:'$isobject(mod)
        set key = "", unit = 0
        for {
            set r = mod.Resources.GetNext(.key)
            quit:key=""
            write !,"RES|"_m_"|"_r.Name_"|"_r.ProcessorClass_"|"_r.Attributes.GetAt("Directory")_"|"_r.Attributes.GetAt("Package")_"|"_r.Attributes.GetAt("Phase"),!
            set isUnit = (r.Name = $case(m, "m1":"/tests/", "m4":"/src/tests/", :"")) && (r.ProcessorClass [ "UnitTest")
            set:isUnit&&(r.Attributes.GetAt("Package") = $case(m, "m1":"It1", :"Ie1T"))&&(r.Attributes.GetAt("Phase") = "test") unit = 1
        }
        do:(m="m1")||(m="m4") ..Obs(m_".loaded.unittest", unit)
    }
    for c="Ia1.B.C","Ia2.D.E","Ia3.B.C","Ia5.Y","Id1.B.C","Id1.B.E","Id2.B.C","Id2.D.E","Ie1.A","Ir2a","Ib2a.P.Q","Ir2e","Ik2b.P.Q","Im7.X","Im7.Y","Im8.X","Im8.Y" {
        // A class declared without a package compiles into the User package.
        set compiled = $select(c[".":c, 1:"User."_c)
        do ..Obs("class."_c, ##class(%Dictionary.CompiledClass).%ExistsId(compiled))
    }
    do ..Seeded("after")
    write !,"NATIVE-END",!
}

}
