Include %IPM.Common
Class Iter26.SyncSummary
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod State(label)
{
 do ..Obs(label_"-active",+$get(^||IPM.Sync.Summary("Active")))
 set key="",count=0
 for {set key=$order(^||IPM.Sync.Summary("Module",key)) quit:key=""  set count=count+1}
 do ..Obs(label_"-module-count",count)
 do ..Obs(label_"-zeta-count",+$get(^||IPM.Sync.Summary("Module","iter26-zeta","SkippedCount")))
 do ..Obs(label_"-alpha-count",+$get(^||IPM.Sync.Summary("Module","iter26-alpha","SkippedCount")))
 set count=0,seq="",order=""
 for {
  set seq=$order(^||IPM.Sync.Summary("Module","iter26-zeta","Skipped",seq),1,class)
  quit:seq=""
  set count=count+1,order=order_$listbuild(seq)
  write !,"STORED|",label,"|iter26-zeta|",seq,"|",class,!
 }
 do ..Obs(label_"-zeta-rows",count)
 do ..Obs(label_"-zeta-class1",$get(^||IPM.Sync.Summary("Module","iter26-zeta","Skipped",1))="Iter26.VerifyZ")
 do ..Obs(label_"-zeta-class2",$get(^||IPM.Sync.Summary("Module","iter26-zeta","Skipped",2))="Iter26.VerifyA")
 do ..Obs(label_"-zeta-class3",$get(^||IPM.Sync.Summary("Module","iter26-zeta","Skipped",3))="Iter26.VerifyZ")
 do ..Obs(label_"-alpha-class1",$get(^||IPM.Sync.Summary("Module","iter26-alpha","Skipped",1))="Iter26.Alpha")
 do ..Obs(label_"-zeta-class1-exists",$data(^||IPM.Sync.Summary("Module","iter26-zeta","Skipped",1))#2)
 do ..Obs(label_"-alpha-class1-exists",$data(^||IPM.Sync.Summary("Module","iter26-alpha","Skipped",1))#2)
}
ClassMethod Report(label,names)
{
 write !,"BEGIN|",label,!
 do ##class(%IPM.General.Sync.Summary).Report(names)
 write !,"END|",label,!
 do ..Obs(label,1)
}
ClassMethod Cmd(label,cmd)
{
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(cmd)
 write !,"END|",label,!
 do ..Obs(label,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"ERROR|",label,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod CLI()
{
 kill ^||Iter26TestCalls
 do ..Cmd("cli-load","load /tmp/iter26/module -dev")
 do ..Obs("cli-initial-marker",##class(Iter26.Verify.Case).Marker())
 do ..Obs("cli-copy-two",##class(%File).CopyFile("/tmp/iter26/replacement/Case2.cls","/tmp/iter26/module/tests/Iter26/Verify/Case.cls"))
 do ..Cmd("cli-single","sync iter26-cli -test -verbose")
 do ..Obs("cli-single-marker",##class(Iter26.Verify.Case).Marker()),..Obs("cli-single-no-test-call",+$get(^||Iter26TestCalls)=0)
 do ..Obs("cli-single-summary-cleared",$data(^||IPM.Sync.Summary)=0)
 do ..Obs("cli-copy-three",##class(%File).CopyFile("/tmp/iter26/replacement/Case3.cls","/tmp/iter26/module/tests/Iter26/Verify/Case.cls"))
 do ..Cmd("cli-all","sync -test -verbose")
 do ..Obs("cli-all-marker",##class(Iter26.Verify.Case).Marker()),..Obs("cli-all-no-test-call",+$get(^||Iter26TestCalls)=0)
 do ..Obs("cli-all-summary-cleared",$data(^||IPM.Sync.Summary)=0)
}
ClassMethod Run()
{
 do ##class(%IPM.General.Sync.Summary).End()
 do ##class(%IPM.General.Sync.Summary).AddSkipped("iter26-zeta","Iter26.VerifyZ")
 do ..State("inactive"),..Report("report-inactive",$listbuild("iter26-zeta","iter26-alpha"))
 do ##class(%IPM.General.Sync.Summary).Begin()
 do ..State("begun")
 do ##class(%IPM.General.Sync.Summary).AddSkipped("iter26-zeta","Iter26.VerifyZ")
 do ..State("a1")
 do ##class(%IPM.General.Sync.Summary).AddSkipped("iter26-alpha","Iter26.Alpha")
 do ..State("b1")
 do ##class(%IPM.General.Sync.Summary).AddSkipped("iter26-zeta","Iter26.VerifyA")
 do ..State("a2")
 do ##class(%IPM.General.Sync.Summary).AddSkipped("iter26-zeta","Iter26.VerifyZ")
 do ..State("a3")
 do ..Report("report-zeta-first",$listbuild("iter26-zeta","iter26-alpha"))
 do ..Report("report-alpha-first",$listbuild("iter26-alpha","iter26-zeta")),..State("after-report")
 do ##class(%IPM.General.Sync.Summary).End()
 do ..State("after-end")
 do ##class(%IPM.General.Sync.Summary).AddSkipped("iter26-alpha","Iter26.Alpha")
 do ..State("after-inactive")
 do ##class(%IPM.General.Sync.Summary).Begin()
 do ..State("reset-empty")
 do ##class(%IPM.General.Sync.Summary).AddSkipped("iter26-zeta","Iter26.VerifyZ")
 do ..State("reset"),..Report("report-reset",$listbuild("iter26-zeta"))
 do ##class(%IPM.General.Sync.Summary).End()
 do ..CLI()
 do ..Obs("workflow-complete",1),..Obs("final-tlevel",$tlevel)
}
}
