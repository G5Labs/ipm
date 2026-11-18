/// Test-only observer for iteration69. Not IPM code.
Class IPMCov.GenReader69
{

Parameter ROOT = "/tmp/iter69/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Text(pCase As %String) As %String
{
    set file = ..#ROOT_pCase_"/module.xml"
    quit:'##class(%File).Exists(file) ""
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(file)
    quit s.Read(1000000)
}

/// Records whether module.xml of pCase contains pNeedle (or, with pCount, how many times).
ClassMethod Has(pKey As %String, pCase As %String, pNeedle As %String, pCount As %Boolean = 0)
{
    set t = ..Text(pCase)
    do ..Obs(pKey, $select(pCount:$length(t, pNeedle) - 1, 1:t[pNeedle))
}

ClassMethod Manifests()
{
    for m="m1","m3","m4","m2a","m2e","m2b","m5","m6" {
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
}

ClassMethod Loaded()
{
    for m="m1","m3","m4","m2a","m2e","m2b" {
        do ..Obs(m_".load.ok", ##class(%IPM.Storage.Module).NameExists(m))
        set mod = ##class(%IPM.Storage.Module).NameOpen(m)
        continue:'$isobject(mod)
        set key = ""
        for {
            set r = mod.Resources.GetNext(.key)
            quit:key=""
            write !,"RES|"_m_"|"_r.Name_"|"_r.ProcessorClass_"|"_r.Attributes.GetAt("Directory"),!
        }
    }
    for c="Ia1.B.C","Ia2.D.E","Ia3.B.C","Ia5.Y","Id1.B.C","Id1.B.E","Id2.B.C","Id2.D.E","Ie1.A","Ir2a","Ib2a.P.Q","Ir2e","Ik2b.P.Q" {
        // A class declared without a package compiles into the User package.
        set compiled = $select(c[".":c, 1:"User."_c)
        do ..Obs("class."_c, ##class(%Dictionary.CompiledClass).%ExistsId(compiled))
    }
    write !,"NATIVE-END",!
}

}
