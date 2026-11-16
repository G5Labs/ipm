Include %IPM.Common
Class Iter11.Concurrent
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"RAW|",k,"|",$system.Status.GetErrorText(sc),!}
 return $$$ISOK(sc)
}
ClassMethod Worker(module,label)
{
 set ^Iter11("label",$job)=label,^Iter11("job",label)=$job
 try {
  set sc=##class(%IPM.Main).Shell("module-action "_module_" ApplyUpdateSteps -only")
  set ^Iter11("ok",label)=$$$ISOK(sc)
  if $$$ISERR(sc) {set ^Iter11("error",label)=$system.Status.GetErrorText(sc)}
 } catch ex {
  set ^Iter11("ok",label)=0,^Iter11("error",label)=ex.DisplayString()
 }
 set ^Iter11("done",label)=1
}
ClassMethod Wait(kind,label,ticks=200) As %Boolean
{
 for i=1:1:ticks {quit:$get(^Iter11(kind,label))  hang 0.05}
 return +$get(^Iter11(kind,label))
}
ClassMethod LockFree(module) As %Boolean
{
 lock +^IPM.UpdateLock(module):0
 set free=$test
 if free {lock -^IPM.UpdateLock(module)}
 return free
}
ClassMethod Row(label)
{
 set s=##class(%IPM.Storage.UpdateStep.AnyMember).GetUpdateStep("Iter11.Steps.V1","Shared")
 do ..Obs(label_"-home-a",s.HomeModule.Name="iter11-a")
 do ..Obs(label_"-home-b",s.HomeModule.Name="iter11-b")
 do ..Obs(label_"-complete",s.TimeStampEnd'="")
 do ..Obs(label_"-ok",$$$ISOK(s.Status))
}
ClassMethod Run()
{
 kill ^Iter11
 do ..Cmd("concurrent-load-a","load /tmp/iter11/a")
 do ..Cmd("concurrent-load-b","load /tmp/iter11/b")
 $$$ThrowOnError($system.OBJ.Load("/tmp/iter11/Steps.cls","ck"))
 job ..Worker("iter11-a","a")
 do ..Obs("a-entered",..Wait("entered","a"))
 do ..Obs("a-lock-free",..LockFree("iter11-a")),..Obs("b-lock-free-before",..LockFree("iter11-b"))
 do ..Row("before-b")
 job ..Worker("iter11-b","b")
 do ..Obs("b-entered",..Wait("entered","b",120))
 do ..Obs("b-lock-free-during",..LockFree("iter11-b"))
 do ..Obs("both-count",+$get(^Iter11("count")))
 set ^Iter11("release","b")=1
 do ..Obs("b-done",..Wait("done","b"))
 do ..Row("after-b")
 set ^Iter11("release","a")=1
 do ..Obs("a-done",..Wait("done","a"))
 do ..Row("after-a")
 do ..Obs("a-status",+$get(^Iter11("ok","a"))),..Obs("b-status",+$get(^Iter11("ok","b")))
 do ..Obs("final-count",+$get(^Iter11("count")))
 do ..Obs("final-value",+$get(^Iter11("value")))
 do ..Obs("a-tlevel",+$get(^Iter11("tlevel","a"))),..Obs("b-tlevel",+$get(^Iter11("tlevel","b")))
 do ..Obs("barrier-timeouts",+$get(^Iter11("timeout","a"))+$get(^Iter11("timeout","b")))
 $$$ThrowOnError(##class(%IPM.Storage.UpdateStep.AnyMember).UpdateClassMethodIdxDelete("Iter11.Steps.V1","Shared"))
 set ^Iter11("count")=0
 job ..Worker("iter11-a","same-a")
 do ..Obs("same-a-entered",..Wait("entered","same-a"))
 job ..Worker("iter11-a","same-b")
 do ..Obs("same-b-done",..Wait("done","same-b",120))
 do ..Obs("same-b-entered",+$get(^Iter11("entered","same-b")))
 do ..Obs("same-count",+$get(^Iter11("count")))
 do ..Obs("same-b-status",+$get(^Iter11("ok","same-b")))
 set ^Iter11("release","same-a")=1
 do ..Obs("same-a-done",..Wait("done","same-a"))
 do ..Obs("same-a-status",+$get(^Iter11("ok","same-a")))
 do ..Obs("same-timeout",+$get(^Iter11("timeout","same-a")))
 $$$ThrowOnError(##class(%IPM.Storage.UpdateStep.AnyMember).UpdateClassMethodIdxDelete("Iter11.Steps.V1","Shared"))
 set ^Iter11("count")=0,^Iter11("value")=0,^Iter11("release","serial-a")=1,^Iter11("release","serial-b")=1
 do ..Worker("iter11-a","serial-a"),..Worker("iter11-b","serial-b")
 do ..Obs("serial-a-entered",+$get(^Iter11("entered","serial-a")))
 do ..Obs("serial-b-entered",+$get(^Iter11("entered","serial-b")))
 do ..Obs("serial-count",+$get(^Iter11("count"))),..Obs("serial-value",+$get(^Iter11("value")))
 do ..Obs("serial-a-status",+$get(^Iter11("ok","serial-a"))),..Obs("serial-b-status",+$get(^Iter11("ok","serial-b")))
 do ..Row("serial-final")
}
}
