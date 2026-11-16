Include %IPM.Common
Class Iter10.ErrorProbe
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) { write !,"RAW|",k,"|",$system.Status.GetErrorText(sc),! }
 return $$$ISOK(sc)
}
ClassMethod Run()
{
 set ^Iter10Fail=0
 do ..Cmd("error-load","load /tmp/iter10/bad")
 set r=##class(%IPM.Storage.ResourceReference).ResourceNameOpen("Iter10.Payload.CLS")
 set sc=r.ResolveChildren(.children)
 do ..Obs("resolve-ok",$$$ISOK(sc)),..Obs("resolve-child",$data(children("Iter10.Payload.CLS"))>0)
 kill r,children
 set ^Iter10Fail=1,^Iter10HookCount=0
 set r=##class(%IPM.Storage.ResourceReference).ResourceNameOpen("Iter10.Payload.CLS")
 set sc=r.ResolveChildren(.children)
 do ..Obs("resolve-fail",$$$ISOK(sc)),..Obs("resolve-empty",$data(children)=0)
 do ..Obs("resolve-hook",^Iter10HookCount>0)
 kill r,children
 set ^Iter10HookCount=0
 set ^Iter10HookCount=0
 do ..Cmd("error-uninstall","uninstall iter10-bad")
 do ..Obs("uninstall-hook",^Iter10HookCount>0)
 do ..Obs("error-module",##class(%IPM.Storage.Module).NameExists("iter10-bad"))
 do ..Obs("error-manifest",##class(%IPM.StudioDocument.ModuleStream).NameExists("iter10-bad"))
 do ..Obs("error-code",##class(%Dictionary.ClassDefinition).%ExistsId("Iter10.Payload"))
 do ..Obs("error-call",$classmethod("Iter10.Payload","Value")=71)
 set ^Iter10Fail=0
 do ..Cmd("valid-reload","load /tmp/iter10/bad")
 do ..Cmd("valid-uninstall","uninstall iter10-bad")
 do ..Obs("valid-module",##class(%IPM.Storage.Module).NameExists("iter10-bad"))
 do ..Obs("valid-code",##class(%Dictionary.ClassDefinition).%ExistsId("Iter10.Payload"))
}
}
