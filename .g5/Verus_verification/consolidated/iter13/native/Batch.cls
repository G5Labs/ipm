Include %IPM.Common
Class Iter13.Batch
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd)
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc)),..Obs(k_"-tlevel",$tlevel)
 if $$$ISERR(sc) {write !,"RAW|",k,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod Maps(label,ns)
{
 new $namespace
 set $namespace="%SYS"
 set pkg=##class(Config.MapPackages).Exists(ns,"%IPM"),rtn=##class(Config.MapRoutines).Exists(ns,"%IPM.*")
 set dest=##class(%SYS.Namespace).GetPackageDest(ns,"%IPM"),source=##class(%SYS.Namespace).GetPackageDest("USER","%IPM")
 set $namespace="USER"
 do ..Obs(label_"-pkg",pkg),..Obs(label_"-rtn",rtn),..Obs(label_"-active",dest=source)
}
ClassMethod Target(ns,label)
{
 set $namespace=ns
 set ^|"USER"|Iter13(label,"callable")=$system.CLS.IsMthd("%IPM.Main","Shell")
 set ^|"USER"|Iter13(label,"shell")=0,^|"USER"|Iter13(label,"exception")=0
 try {
  set sc=##class(%IPM.Main).Shell("version")
  set ^|"USER"|Iter13(label,"shell")=$$$ISOK(sc)
 } catch ex {
  set ^|"USER"|Iter13(label,"exception")=1
  set ^|"USER"|Iter13(label,"error")=ex.DisplayString()
 }
 set ^|"USER"|Iter13(label,"done")=1
}
ClassMethod Read(ns,label)
{
 job ..Target(ns,label)
 for i=1:1:300 {quit:$get(^Iter13(label,"done"))  hang 0.05}
 for f="done","callable","shell","exception" {do ..Obs(label_"-"_f,+$get(^Iter13(label,f)))}
 if $data(^Iter13(label,"error")) {write !,"TARGET-ERROR|",label,"|",^Iter13(label,"error"),!}
}
ClassMethod Run()
{
 kill ^Iter13
 // Prepare every namespace before creating any intentionally pending mapping.
 for ns="ITER13P","ITER13F","ITER13L","ITER13U","ITER13V" {
  $$$ThrowOnError(##class(%IPM.Test.Utils).CreateNamespace(ns))
  $$$ThrowOnError(##class(%IPM.Utils.Module).RemovePackageMapping(ns,"%IPM"))
  do ..Maps("empty-"_ns,ns)
 }
 do ..Cmd("preview","enable -map -ns ITER13P,ITER13MISSING -preview")
 do ..Maps("preview","ITER13P"),..Read("ITER13P","preview")
 do ..Cmd("first-bad","enable -map -ns ITER13MISSING,ITER13F")
 do ..Maps("first-bad","ITER13F"),..Read("ITER13F","first-bad")
 do ..Cmd("late-bad","enable -map -ns ITER13L,ITER13MISSING")
 do ..Maps("stranded","ITER13L"),..Read("ITER13L","stranded")
 do ..Cmd("retry","enable -map -ns ITER13L")
 do ..Maps("retry","ITER13L"),..Read("ITER13L","retry")
 do ..Cmd("late-other","enable -map -ns ITER13U,ITER13MISSING")
 do ..Maps("pending-other","ITER13U"),..Read("ITER13U","pending-other")
 do ..Cmd("valid","enable -map -ns ITER13V")
 do ..Maps("valid","ITER13V"),..Read("ITER13V","valid")
 do ..Maps("after-other","ITER13U"),..Read("ITER13U","after-other")
 do ..Maps("preview-final","ITER13P"),..Maps("first-final","ITER13F")
}
}
