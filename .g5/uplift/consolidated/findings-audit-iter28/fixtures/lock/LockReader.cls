Class FindingsAudit.LockReader
{
ClassMethod Run()
{
 do ##class(%File).Delete("module-lock.json")
 set sc=##class(%IPM.Main).Shell("load /tmp/uplift/lock/module -lock")
 write !,"LOCK|plain-ok|",$system.Status.IsOK(sc),!
 set m=##class(%IPM.Storage.Module).NameOpen("audit-lock")
 write "LOCK|plain-root-present|",(##class(%IPM.Storage.Module).RootGetStored(m.%Id())'=""),!
 write "LOCK|cwd-file|",##class(%File).Exists("module-lock.json"),!
 write "LOCK|source-file|",##class(%File).Exists("/tmp/uplift/lock/module/module-lock.json"),!
 set f=##class(%Stream.FileCharacter).%New() do f.LinkToFile("module-lock.json")
 set obj=##class(%DynamicObject).%FromJSON(f.Read(32000))
 write "LOCK|json-name|",obj.name,!
 kill m
 set sc=##class(%IPM.Main).Shell("load /tmp/uplift/lock/module -lock -dev")
 write "LOCK|dev-ok|",$system.Status.IsOK(sc),!
 write "LOCK|dev-source-file|",##class(%File).Exists("/tmp/uplift/lock/module/module-lock.json"),!
}
}
