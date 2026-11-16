Include %IPM.Common
Class Iter11.DeleteProbe
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"RAW|",k,"|",$system.Status.GetErrorText(sc),!}
 return $$$ISOK(sc)
}
ClassMethod Holder()
{
 try {
  set sc=##class(%Dictionary.ClassDefinition).%LockId("Iter11Delete.Item",0,2)
  set ^Iter11Delete("acquired")=$$$ISOK(sc)
  if $$$ISOK(sc) {
   for i=1:1:900 {quit:$get(^Iter11Delete("release"))  hang 0.05}
   set ^Iter11Delete("deadline")='$get(^Iter11Delete("release"))
   set sc=##class(%Dictionary.ClassDefinition).%UnlockId("Iter11Delete.Item",0,1)
  }
 } catch ex {set ^Iter11Delete("error")=ex.DisplayString()}
 set ^Iter11Delete("done")=1
}
ClassMethod Run()
{
 kill ^Iter11Delete
 do ..Cmd("delete-load","load /tmp/iter11/delete")
 job ..Holder()
 for i=1:1:80 {quit:$data(^Iter11Delete("acquired"))!$get(^Iter11Delete("done"))  hang 0.05}
 set acquired=+$get(^Iter11Delete("acquired"))
 do ..Obs("delete-lock-acquired",acquired)
 if acquired {
  set sc=$system.OBJ.Delete("Iter11Delete.Item","-d")
  do ..Obs("delete-direct-status",$$$ISOK(sc))
  do ..Obs("delete-direct-retained",##class(%Dictionary.ClassDefinition).%ExistsId("Iter11Delete.Item"))
  if $$$ISERR(sc),##class(%Dictionary.ClassDefinition).%ExistsId("Iter11Delete.Item") {
   do ..Cmd("delete-locked-uninstall","uninstall iter11-delete")
   do ..Obs("delete-locked-module",##class(%IPM.Storage.Module).NameExists("iter11-delete"))
   do ..Obs("delete-locked-manifest",##class(%IPM.StudioDocument.ModuleStream).NameExists("iter11-delete"))
   do ..Obs("delete-locked-code",##class(%Dictionary.ClassDefinition).%ExistsId("Iter11Delete.Item"))
  }
 }
 set ^Iter11Delete("release")=1
 for i=1:1:100 {quit:$get(^Iter11Delete("done"))  hang 0.05}
 do ..Obs("delete-holder-done",+$get(^Iter11Delete("done")))
 do ..Obs("delete-holder-deadline",+$get(^Iter11Delete("deadline")))
 if ##class(%Dictionary.ClassDefinition).%ExistsId("Iter11Delete.Item") {do ..Obs("delete-call",$classmethod("Iter11Delete.Item","Value")=73)}
 do ..Cmd("delete-unlocked-reload","load /tmp/iter11/delete")
 do ..Cmd("delete-unlocked-uninstall","uninstall iter11-delete")
 do ..Obs("delete-unlocked-code",##class(%Dictionary.ClassDefinition).%ExistsId("Iter11Delete.Item"))
}
}
