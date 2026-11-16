/// Test-only observer for iteration64. Not IPM code.
Class IPMCont.FailReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Runs in %SYS without calling back into this class.
ClassMethod MakeDB() As %Status
{
    new $namespace
    set $namespace = "%SYS"
    set dir = "/usr/irissys/mgr/db64/"
    do ##class(%File).CreateDirectoryChain(dir)
    set sc = ##class(SYS.Database).CreateDatabase(dir)
    quit:'sc sc
    kill p set p("Directory") = dir
    quit ##class(Config.Databases).Create("DB64",.p)
}

/// $lb(webapp, globalMap, packageMap, cpfPackageMap), read in %SYS.
ClassMethod SysState(pC As %String, pc As %String) As %List
{
    new $namespace
    set $namespace = "%SYS"
    quit $lb(##class(Security.Applications).Exists("/ipmcont64"_pc),##class(Config.MapGlobals).Exists("USER","G"_pC_"64"),##class(Config.MapPackages).Exists("USER","P"_pC_"64"),##class(Config.MapPackages).Exists("USER","C"_pC_"64"))
}

ClassMethod CountMsgXml(pDir As %String) As %Integer
{
    set n = 0
    quit:'##class(%File).DirectoryExists(pDir) 0
    set rs = ##class(%ResultSet).%New("%File:FileSet")
    do rs.Execute(pDir,"*")
    while rs.Next() {
        set name = rs.Get("Name"), type = rs.Get("Type")
        if type = "D" { set n = n + ..CountMsgXml(name) }
        else {
            set f = $zconvert(##class(%File).GetFilename(name),"L")
            if ($extract(f,*-3,*) = ".xml") && (f '= "module.xml") { set n = n + 1 }
        }
    }
    quit n
}

ClassMethod State(pCase As %String, pc As %String)
{
    set C = $zconvert(pc,"U"), m = "m64"_pc, root = "/tmp/iter64/"_m_"/"
    do ..Obs(pCase_".record",##class(%IPM.Storage.Module).NameExists(m))
    do ..Obs(pCase_".good",##class(%Dictionary.CompiledMethod).%ExistsId("X"_C_"64.Good||Get"))
    do ..Obs(pCase_".gooddef",##class(%Dictionary.ClassDefinition).%ExistsId("X"_C_"64.Good"))
    do ..Obs(pCase_".filea",##class(%File).Exists("/tmp/iter64t/"_pc_"/a.txt"))
    do ..Obs(pCase_".fileb",##class(%File).Exists("/tmp/iter64t/"_pc_"/dir/b.txt"))
    set s = ..SysState(C,pc)
    do ..Obs(pCase_".webapp",$list(s,1))
    do ..Obs(pCase_".mapg",$list(s,2))
    do ..Obs(pCase_".mapp",$list(s,3))
    do ..Obs(pCase_".cpf",$list(s,4))
    do ..Obs(pCase_".msg",$data(^IRIS.Msg("D"_C_"64","en","A"))#2)
    do ..Obs(pCase_".locdoc",##class(%RoutineMgr).Exists("L"_C_"64.Msgs.LOC"))
    do ..Obs(pCase_".locexport",..CountMsgXml(root))
    do ..Obs(pCase_".sources",$data(^Sources("MODULE",m))#2)
    set rs = ##class(%SQL.Statement).%ExecDirect(,"SELECT ID FROM %IPM_General.History WHERE Package = ? ORDER BY ID DESC",m)
    set hid = $select(rs.%Next():rs.%Get("ID"),1:"")
    set h = $select(hid="":"",1:##class(%IPM.General.History).%OpenId(hid))
    do ..Obs(pCase_".hist",$isobject(h))
    do ..Obs(pCase_".hist.success",$select($isobject(h):$system.Status.IsOK(h.Success),1:-1))
    do ..Obs(pCase_".hist.committed",$select($isobject(h):h.Committed,1:-1))
}

ClassMethod Load(pCase As %String, pc As %String, pFlags As %String = "")
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell("load /tmp/iter64/m64"_pc_" "_pFlags)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    for i=1:1:$get(lines) { write !,"OUT|",pCase,"|",$zstrip($get(lines(i)),"*C")
        set:lines(i)["Activate FAILURE" fa = 1
        set:lines(i)["Compile FAILURE" fc = 1 }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".failcompile",$get(fc))
    do ..Obs(pCase_".failactivate",$get(fa))
    do ..Obs(pCase_".tlevel",$tlevel)
    do ..State(pCase,pc)
}

ClassMethod All()
{
    set sc = ..MakeDB()
    do ..Obs("setup.db",$system.Status.IsOK(sc))
    do ..Load("ok","ok")
    do ..Load("ec","ec")
    do ..Load("la","la")
    do ..Load("dv","dv","-dev")
    write !,"NATIVE-END",!
}

}
