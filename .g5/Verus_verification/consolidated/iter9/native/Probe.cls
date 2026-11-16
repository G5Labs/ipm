Include %IPM.Common
Class Iter9.Probe
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) { write !,"RAW|",k,"|",$system.Status.GetErrorText(sc),! }
 return $$$ISOK(sc)
}
ClassMethod Packages()
{
 set base="/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/"
 do ..Cmd("pkg-load", "load "_base_"pkg-case-test")
 do ..Cmd("extra-load", "load "_base_"pkg-case-extra")
 do ..Cmd("nested-load", "load /tmp/iter9/nested")
 set r=##class(%IPM.Storage.ResourceReference).ResourceNameOpen("Test.PKGCASE.PKG")
 $$$ThrowOnError(r.ResolveChildren(.all,0))
 do ..Obs("unchecked-extra",$data(all("Test.PkgCase.ExtraClass.cls"))>0)
 $$$ThrowOnError(r.ResolveChildren(.selected))
 do ..Obs("checked-extra",$data(selected("Test.PkgCase.ExtraClass.cls"))>0)
 do ..Obs("checked-inner",$data(selected("Test.PkgCase.Nested.Inner.cls"))>0)
 do ..Cmd("pkg-uninstall", "uninstall pkg-case-test")
 do ..Obs("pkg-row",##class(%IPM.Storage.Module).NameExists("pkg-case-test"))
 do ..Obs("own-class",##class(%Dictionary.ClassDefinition).%ExistsId("Test.PkgCase.MyClass"))
 do ..Obs("extra-class",##class(%Dictionary.ClassDefinition).%ExistsId("Test.PkgCase.ExtraClass"))
 do ..Obs("inner-class",##class(%Dictionary.ClassDefinition).%ExistsId("Test.PkgCase.Nested.Inner"))
 do ..Obs("inner-call",$classmethod("Test.PkgCase.Nested.Inner","Read")=92)
 do ..Cmd("extra-uninstall", "uninstall pkg-case-extra")
 do ..Obs("extra-removed",'##class(%Dictionary.ClassDefinition).%ExistsId("Test.PkgCase.ExtraClass"))
 do ..Cmd("same-load","load /tmp/iter9/same")
 set r=##class(%IPM.Storage.ResourceReference).ResourceNameOpen("Test.PKGCASE.PKG")
 kill selected
 $$$ThrowOnError(r.ResolveChildren(.selected))
 do ..Obs("same-parent-child",$data(selected("Test.PkgCase.MyClass.cls"))>0)
 do ..Cmd("same-uninstall","uninstall iter9-same")
 do ..Obs("same-class",##class(%Dictionary.ClassDefinition).%ExistsId("Test.PkgCase.MyClass"))
}
ClassMethod Steps()
{
 do ..Cmd("step-a-load", "load /tmp/iter9/step-a")
 set any=##class(%IPM.Storage.UpdateStep.AnyMember).GetUpdateStep("Iter9.Steps.V1","A")
 set primary=##class(%IPM.Storage.UpdateStep.PrimaryOnly).GetUpdateStep("Iter9.Steps.V1","P")
 do ..Obs("seed-any",any.TimeStampEnd'=""),..Obs("seed-primary",primary.TimeStampEnd'="")
 do ..Obs("step-not-run",$get(^Iter9Ran,0))
 do ..Obs("seed-home-a",any.HomeModule.Name="iter9-step-a")
 kill any,primary
 do ..Cmd("step-a-first-uninstall","uninstall iter9-step-a")
 do ..Obs("any-first-delete",##class(%IPM.Storage.UpdateStep.AnyMember).UpdateClassMethodIdxExists("Iter9.Steps.V1","A"))
 do ..Obs("primary-first-delete",##class(%IPM.Storage.UpdateStep.PrimaryOnly).UpdateClassMethodIdxExists("Iter9.Steps.V1","P"))
 do ..Cmd("step-a-reload","load /tmp/iter9/step-a")
 do ..Cmd("step-b-load", "load /tmp/iter9/step-b")
 set any=##class(%IPM.Storage.UpdateStep.AnyMember).GetUpdateStep("Iter9.Steps.V1","A")
 do ..Obs("home-moved",any.HomeModule.Name="iter9-step-b"),..Obs("moved-completed",any.TimeStampEnd'="")
 kill any
 do ..Cmd("step-a-uninstall", "uninstall iter9-step-a")
 do ..Obs("any-after-a",##class(%IPM.Storage.UpdateStep.AnyMember).UpdateClassMethodIdxExists("Iter9.Steps.V1","A"))
 do ..Obs("primary-after-a",##class(%IPM.Storage.UpdateStep.PrimaryOnly).UpdateClassMethodIdxExists("Iter9.Steps.V1","P"))
 do ..Obs("b-after-a",##class(%IPM.Storage.Module).NameExists("iter9-step-b"))
 do ..Cmd("step-b-uninstall", "uninstall iter9-step-b")
 do ..Obs("any-after-b",##class(%IPM.Storage.UpdateStep.AnyMember).UpdateClassMethodIdxExists("Iter9.Steps.V1","A"))
 do ..Obs("primary-after-b",##class(%IPM.Storage.UpdateStep.PrimaryOnly).UpdateClassMethodIdxExists("Iter9.Steps.V1","P"))
}
}
