/// Test-only observer for iteration53. Not IPM code.
Class IPMCont.GenReader
{

Parameter ROOT = "/tmp/iter53/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Setup()
{
    set sc = $system.OBJ.LoadDir(..#ROOT_"defs/","ck-d")
    for r="G53E.Rout","G53E.Skip.R","ZZG53E" { set sc = $system.Status.AppendStatus(sc,$system.OBJ.Load(..#ROOT_"defs/"_r_".mac","ck-d")) }
    do ..Obs("setup.defs",$system.Status.IsOK(sc))
    do ..Obs("setup.routines",##class(%RoutineMgr).Exists("G53E.Rout.MAC")+##class(%RoutineMgr).Exists("G53E.Skip.R.MAC")+##class(%RoutineMgr).Exists("ZZG53E.MAC"))
    do ##class(%IPM.Storage.ModuleTemplate).GetCSPApplications(.apps)
    write !,"APPS|",$listtostring(apps,","),!
    do ..Obs("setup.apps",$listlength(apps))
}

ClassMethod Text(pFile As %String) As %String
{
    quit:'##class(%File).Exists(pFile) ""
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(pFile)
    quit s.Read(1000000)
}

ClassMethod Has(pCase As %String, pKey As %String, pText As %String, pNeedle As %String)
{
    do ..Obs(pCase_"."_pKey,pText[pNeedle)
}

ClassMethod After()
{
    set t1 = ..Text(..#ROOT_"g1/module.xml")
    write !,"XML1|",$translate(t1,$c(10,13),"  "),!
    do ..Obs("t1.exists",t1'="")
    do ..Has("t1","name",t1,"<Name>template</Name>")
    do ..Has("t1","version",t1,"<Version>1.0.0</Version>")
    do ..Has("t1","license",t1,"<License>MIT</License>")
    do ..Has("t1","copyright",t1,"<CopyrightDate>"_$piece($zdate($h,3),"-")_"</CopyrightDate>")
    do ..Has("t1","dep",t1,"<Name>module</Name>")
    set t2 = ..Text(..#ROOT_"g2/module.xml")
    write !,"XML2|",$translate(t2,$c(10,13),"  "),!
    do ..Obs("t2.exists",t2'="")
    do ..Has("t2","name",t2,"<Name>g53</Name>")
    do ..Has("t2","version",t2,"<Version>2.0.0</Version>")
    do ..Has("t2","licenseIsLicense",t2,"<License>Apache-2.0</License>")
    do ..Has("t2","licenseIsYear",t2,"<License>2024</License>")
    do ..Has("t2","copyrightIsYear",t2,"<CopyrightDate>2024</CopyrightDate>")
    do ..Has("t2","copyrightIsLicense",t2,"<CopyrightDate>Apache-2.0</CopyrightDate>")
    do ..Has("t2","person",t2,"<Person>Ann</Person>")
    do ..Has("t2","pkgG53",t2,"Name=""G53.PKG""")
    do ..Has("t2","pkgG53A",t2,"Name=""G53.A.PKG""")
    do ..Has("t2","pkgOther",t2,"Name=""Other.PKG""")
    do ..Has("t2","unittest",t2,"<UnitTest")
    do ..Has("t2","testsAsPkg",t2,"Name=""G53T.PKG""")
    do ..Has("t2","dep",t2,"<Name>g53dep</Name>")
    do ..Has("t2","depver",t2,"<Version>1.x</Version>")
    set e = ..#ROOT_"g3/src/"
    do ..Obs("e1.keep",##class(%File).Exists(e_"cls/G53E/Keep.cls"))
    do ..Obs("e1.skipcls",##class(%File).Exists(e_"cls/G53E/Skip/Gone.cls"))
    do ..Obs("e1.rout",##class(%File).Exists(e_"mac/G53E.Rout.mac"))
    do ..Obs("e1.skiprout",##class(%File).Exists(e_"mac/G53E.Skip.R.mac"))
    do ..Obs("e1.zz",##class(%File).Exists(e_"mac/ZZG53E.mac"))
    set t3 = ..Text(..#ROOT_"g3/module.xml")
    write !,"XML3|",$translate(t3,$c(10,13),"  "),!
    do ..Obs("t3.exists",t3'="")
    write !,"NATIVE-END",!
}

}
