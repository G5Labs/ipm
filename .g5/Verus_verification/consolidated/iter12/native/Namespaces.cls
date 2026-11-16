Include %IPM.Common
Class Iter12.Namespaces
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"RAW|",k,"|",$system.Status.GetErrorText(sc),!}
 return $$$ISOK(sc)
}
ClassMethod Maps(label,ns)
{
 new $namespace
 set $namespace="%SYS"
 set pkg=##class(Config.MapPackages).Exists(ns,"%IPM")
 set rtn=##class(Config.MapRoutines).Exists(ns,"%IPM.*")
 set repo=##class(Config.MapGlobals).Exists(ns,"IPM.Repo.DefinitionD")
 set repos=##class(Config.MapGlobals).Exists(ns,"IPM.Repo.DefinitionS"),repoi=##class(Config.MapGlobals).Exists(ns,"IPM.Repo.DefinitionI")
 set dest=##class(%SYS.Namespace).GetPackageDest(ns,"%IPM")
 set source=##class(%SYS.Namespace).GetPackageDest("USER","%IPM")
 set $namespace="USER"
 do ..Obs(label_"-pkg",pkg),..Obs(label_"-rtn",rtn),..Obs(label_"-repo",repo)
 do ..Obs(label_"-repo-s",repos),..Obs(label_"-repo-i",repoi)
 do ..Obs(label_"-active-source",dest=source)
}
ClassMethod Target(ns,label,action="")
{
 set $namespace=ns
 try {
  set sc=##class(%IPM.Main).Shell("version")
  set ^|"USER"|Iter12(label,"shell")=$$$ISOK(sc)
  if action="create-own" {set own=##class(%IPM.Main).Shell("repo -fs -name iter12-local -path /tmp/iter12/requirements"),^|"USER"|Iter12(label,"action")=$$$ISOK(own)}
  if action="clear-own" {set own=##class(%IPM.Main).Shell("repo -delete-all"),^|"USER"|Iter12(label,"action")=$$$ISOK(own)}
  set ^|"USER"|Iter12(label,"own-visible")=##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("iter12-local")
  set ^|"USER"|Iter12(label,"repo-visible")=##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("iter12-visible")
  set q=##class(%IPM.Storage.SystemRequirements).%New(),q.IPMVersion=">=0.1.0"
  set sc=q.CheckIPMVersion()
  set ^|"USER"|Iter12(label,"requirement")=$$$ISOK(sc)
  set sc=##class(%IPM.Main).Shell("load /tmp/iter12/requirements -nodev")
  set ^|"USER"|Iter12(label,"load")=$$$ISOK(sc)
  set ^|"USER"|Iter12(label,"row")=##class(%IPM.Storage.Module).NameExists("iter12-required")
 } catch ex {
  set ^|"USER"|Iter12(label,"exception")=ex.DisplayString()
 }
 set ^|"USER"|Iter12(label,"done")=1
}
ClassMethod ReadTarget(ns,label,action="")
{
 job ..Target(ns,label,action)
 for i=1:1:300 {quit:$get(^Iter12(label,"done"))  hang 0.05}
 do ..Obs(label_"-done",+$get(^Iter12(label,"done")))
 do ..Obs(label_"-exception",$data(^Iter12(label,"exception"))>0)
 if action'="" {do ..Obs(label_"-action",+$get(^Iter12(label,"action")))}
 for f="shell","own-visible","repo-visible","requirement","load","row" {do ..Obs(label_"-"_f,+$get(^Iter12(label,f)))}
 if $data(^Iter12(label,"exception")) {write !,"TARGET-ERROR|",^Iter12(label,"exception"),!}
}
ClassMethod Run()
{
 kill ^Iter12
 do ..Cmd("visible-repo","repo -fs -name iter12-visible -path /tmp/iter12/requirements")
 $$$ThrowOnError(##class(%IPM.Test.Utils).CreateNamespace("ITER12A"))
 $$$ThrowOnError(##class(%IPM.Utils.Module).RemovePackageMapping("ITER12A","%IPM"))
 do ..Maps("empty","ITER12A")
 do ..Cmd("repos-without-map","enable -repos -ns ITER12A")
 do ..Maps("invalid","ITER12A")
 do ..Cmd("map-preview","enable -map -repos -ns ITER12A -preview")
 do ..Maps("preview","ITER12A")
 do ..Cmd("map-first","enable -map -repos -ns ITER12A")
 do ..Maps("first","ITER12A")
 set q=##class(%IPM.Storage.SystemRequirements).%New(),q.IPMVersion=">=0.1.0"
 do ..Obs("local-requirement",$$$ISOK(q.CheckIPMVersion()))
 do ..Cmd("local-load","load /tmp/iter12/requirements -nodev")
 do ..Obs("local-row",##class(%IPM.Storage.Module).NameExists("iter12-required"))
 do ..Cmd("local-uninstall","uninstall iter12-required")
 do ..ReadTarget("ITER12A","mapped")
 do ..ReadTarget("ITER12A","own-created","create-own")
 do ..Cmd("map-with-own","enable -map -repos -ns ITER12A")
 do ..Maps("own-skip","ITER12A")
 do ..ReadTarget("ITER12A","own-skip")
 do ..ReadTarget("ITER12A","own-cleared","clear-own")
 do ..Cmd("map-second","enable -map -repos -ns ITER12A")
 do ..Maps("second","ITER12A")
 do ..ReadTarget("ITER12A","shared")
 do ..Cmd("repos-unmap","unmap -ns ITER12A -repos-only")
 do ..Maps("repos-only","ITER12A")
 do ..ReadTarget("ITER12A","repos-removed")
 do ..Cmd("map-third","enable -map -repos -ns ITER12A")
 do ..Cmd("full-unmap","unmap -ns ITER12A")
 do ..Maps("unmapped","ITER12A")
 do ..ReadTarget("ITER12A","after-unmap")
}
}
