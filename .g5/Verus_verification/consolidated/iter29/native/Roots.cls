Include %IPM.Common
Class Iter29.Roots
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Raw()
{
 new $namespace
 set $namespace="%SYS"
 return $get(^IPM.settings("ModuleRoot"))
}
ClassMethod State(label,target,old)
{
 set normalized=##class(%File).NormalizeDirectory(target)
 set stored=##class(%IPM.Repo.UniversalSettings).GetValue("ModuleRoot"),root=##class(%IPM.Repo.UniversalSettings).GetModuleRoot(),raw=..Raw()
 write !,"ROOT|",label,"|",stored,"|",root,"|",raw,!
 do ..Obs(label_"-normalized-empty",normalized="")
 do ..Obs(label_"-directory",##class(%File).DirectoryExists(normalized))
 do ..Obs(label_"-writeable",##class(%File).Writeable(normalized))
 do ..Obs(label_"-target",root=normalized)
 do ..Obs(label_"-old",root=old)
 do ..Obs(label_"-stored-target",stored=normalized)
 do ..Obs(label_"-stored-old",stored=old)
 do ..Obs(label_"-raw-stored",raw=stored)
 do ..Obs(label_"-get-stored",root=stored)
}
ClassMethod Case(label,target,shell)
{
 set old=##class(%IPM.Repo.UniversalSettings).GetModuleRoot()
 do ..State(label_"-before",target,old)
 write !,"BEGIN|",label,!
 if shell {set sc=##class(%IPM.Main).Shell("config set ModuleRoot "_target)}
 else {set sc=##class(%IPM.Repo.UniversalSettings).UpdateOne("ModuleRoot",target)}
 write !,"END|",label,!
 do ..Obs(label_"-ok",$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"CALL-ERROR|",label,"|",$system.Status.GetErrorText(sc),!}
 do ..State(label_"-after",target,old)
}
ClassMethod Run()
{
 set sc=##class(%IPM.Repo.UniversalSettings).SetValue("ModuleRoot","")
 do ..Obs("fallback-clear",$$$ISOK(sc)),..Obs("fallback-stored-empty",##class(%IPM.Repo.UniversalSettings).GetValue("ModuleRoot")="")
 do ..Obs("fallback-root",##class(%IPM.Repo.UniversalSettings).GetModuleRoot()=##class(%File).NormalizeDirectory(##class(%SYSTEM.Util).DataDirectory()_"ipm/"))
 do ..Obs("fallback-raw-empty",..Raw()="")
 do ..Obs("parent-directory",##class(%File).DirectoryExists("/tmp/iter29")),..Obs("parent-writeable",##class(%File).Writeable("/tmp/iter29"))
 do ..Obs("readonly-directory",##class(%File).DirectoryExists("/tmp/iter29/read-only")),..Obs("readonly-writeable",##class(%File).Writeable("/tmp/iter29/read-only"))
 kill rc
 set ok=##class(%File).CreateDirectoryChain("/tmp/iter29/calibration/new/",.rc)
 write !,"SDK-CREATE|success|",ok,"|",$get(rc),!
 do ..Obs("sdk-created-ok",ok),..Obs("sdk-created-directory",##class(%File).DirectoryExists("/tmp/iter29/calibration/new/")),..Obs("sdk-created-writeable",##class(%File).Writeable("/tmp/iter29/calibration/new/"))
 kill rc
 set ok=##class(%File).CreateDirectoryChain("/tmp/iter29/blocker/",.rc)
 write !,"SDK-CREATE|obstruction|",ok,"|",$get(rc),!
 do ..Obs("sdk-obstruction-ok",ok),..Obs("sdk-obstruction-directory",##class(%File).DirectoryExists("/tmp/iter29/blocker/")),..Obs("sdk-obstruction-file",##class(%File).Exists("/tmp/iter29/blocker"))
 kill rc
 set ok=##class(%File).CreateDirectoryChain("/tmp/iter29/read-only/calibration-denied/",.rc)
 write !,"SDK-CREATE|parent-denied|",ok,"|",$get(rc),!
 do ..Obs("sdk-denied-ok",ok),..Obs("sdk-denied-directory",##class(%File).DirectoryExists("/tmp/iter29/read-only/calibration-denied/")),..Obs("sdk-denied-parent-writeable",##class(%File).Writeable("/tmp/iter29/read-only/"))
 do ..Case("created-shell","/tmp/iter29/custom/new/nested",1)
 do ..Case("existing-direct","/tmp/iter29/existing",0)
 do ..Case("readonly-direct","/tmp/iter29/read-only",0)
 do ..Case("readonly-shell","/tmp/iter29/read-only",1)
 do ..Case("parent-denied-direct","/tmp/iter29/read-only/child",0)
 do ..Case("parent-denied-shell","/tmp/iter29/read-only/child",1)
 set old=##class(%IPM.Repo.UniversalSettings).GetModuleRoot()
 set sc=##class(%IPM.Repo.UniversalSettings).UpdateOne("ModuleRoot","")
 do ..Obs("empty-direct-ok",$$$ISOK(sc)),..Obs("empty-direct-old",##class(%IPM.Repo.UniversalSettings).GetModuleRoot()=old),..Obs("empty-direct-raw",..Raw()=old)
 do ..Obs("workflow-complete",1),..Obs("final-tlevel",$tlevel),..Obs("namespace-user",$namespace="USER")
}
}
