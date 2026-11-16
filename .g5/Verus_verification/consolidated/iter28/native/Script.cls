Include %IPM.Common
Class Iter28.Script
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Raw()
{
 new $namespace
 set $namespace="%SYS"
 return +$get(^IPM.settings("DefaultLogEntryLimit"))
}
ClassMethod Case(label,file,dry)
{
 set sc=##class(%IPM.Repo.UniversalSettings).SetValue("DefaultLogEntryLimit",7)
 do ..Obs(label_"-reset",$$$ISOK(sc))
 kill ^||Iter28RuntimeCalls
 set cmd="run-from-file /tmp/iter28/files/"_file_$select(dry:" -dry-run",1:"")
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(cmd)
 write !,"END|",label,!
 do ..Obs(label_"-ok",$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"COMMAND-ERROR|",label,"|",$system.Status.GetErrorText(sc),!}
 do ..Obs(label_"-value",+##class(%IPM.Repo.UniversalSettings).GetValue("DefaultLogEntryLimit"))
 do ..Obs(label_"-raw-value",..Raw())
 do ..Obs(label_"-runtime-calls",+$get(^||Iter28RuntimeCalls))
 do ..Obs(label_"-namespace",$namespace="USER")
}
ClassMethod Run()
{
 do ..Case("txt-valid","valid.txt",0)
 do ..Case("txt-unknown","unknown.txt",0)
 do ..Case("txt-dry-unknown","unknown.txt",1)
 do ..Case("txt-runtime","runtime.txt",0)
 do ..Case("json-valid","valid.json",0)
 do ..Case("json-unknown","unknown.json",0)
 do ..Case("json-runtime","runtime.json",0)
 do ..Case("json-dry-valid","valid.json",1)
 do ..Case("json-dry-unknown","unknown.json",1)
 do ..Case("json-array","array.json",0)
 do ..Case("json-wrongcommands","wrongcommands.json",0)
 do ..Case("missing-file","missing.txt",0)
 do ..Case("unsupported-extension","unsupported.text",0)
 do ..Obs("workflow-complete",1),..Obs("final-tlevel",$tlevel)
}
}
