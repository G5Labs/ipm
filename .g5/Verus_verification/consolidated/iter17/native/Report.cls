Include %IPM.Common
Class Iter17.Report
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(label,command)
{
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(command)
 write !,"END|",label,!
 do ..Obs(label,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"STATUS|",label,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod File(label,path)
{
 do ..Obs(label_"-file-exists",##class(%File).Exists(path))
 if '##class(%File).Exists(path) {return}
 set s=##class(%Stream.FileCharacter).%New()
 set sc=s.LinkToFile(path)
 do ..Obs(label_"-file-open",$$$ISOK(sc))
 if $$$ISERR(sc) {return}
 write !,"FILE|",label,"|",s.Read(1000000),!
}
ClassMethod RunOne(label,method)
{
 set before=+$order(^UnitTest.Result(""),-1)
 write !,"BEFORE|",label,"|",before,!
 do ..Cmd(label,"test iter17-outer -only -f json -output-file /tmp/iter17/"_label_".json -DUnitTest.Case=Iter17.Outer.Checks -DUnitTest.Method="_method)
 set latest=+$order(^UnitTest.Result(""),-1)
 write !,"LATEST|",label,"|",latest,!
 do ..File(label,"/tmp/iter17/"_label_".json")
 set id=before
 for {
  set id=$order(^UnitTest.Result(id)) quit:id=""
  write !,"TREE|",label,"|",id,"|",##class(%IPM.Test.Abstract).BuildResultTree(id).%ToJSON(),!
  write !,"RAW-RESULT|",label,"|",id,!
  zwrite ^UnitTest.Result(id)
  set suite=""
  for {
   set suite=$order(^UnitTest.Result(id,suite)) quit:suite=""
   set cls=""
   for {
    set cls=$order(^UnitTest.Result(id,suite,cls)) quit:cls=""
    set meth=""
    for {
     set meth=$order(^UnitTest.Result(id,suite,cls,meth)) quit:meth=""
     set row=^UnitTest.Result(id,suite,cls,meth)
     write !,"METHOD|",label,"|",id,"|",cls,"|",meth,"|",$listget(row,1),"|",$listget(row,3),"|",$listget(row,4),!
     set a=""
     for {
      set a=$order(^UnitTest.Result(id,suite,cls,meth,a)) quit:a=""
      set row=^UnitTest.Result(id,suite,cls,meth,a)
      write !,"ROW|",label,"|",id,"|",cls,"|",meth,"|",a,"|",$listget(row,1),"|",$listget(row,2),"|",$listget(row,3),"|",$listget(row,4),!
     }
     if (cls="Iter17.Outer.Checks")&&(meth="TestNestedFail") {
      set outer=id
     }
    }
   }
  }
 }
 if $data(outer) {
  set sc=##class(%IPM.Test.JsonOutput).ToFile("/tmp/iter17/explicit-outer.json",outer)
  do ..Obs("explicit-outer-ok",$$$ISOK(sc))
  do ..File("explicit-outer","/tmp/iter17/explicit-outer.json")
 }
}
ClassMethod Run()
{
 do ..Cmd("load-child","load /tmp/iter17/child")
 do ..Cmd("load-outer","load /tmp/iter17/outer")
 do ..RunOne("pass","TestPass")
 do ..RunOne("fail","TestFail")
 do ..RunOne("nested","TestNestedFail")
 do ..File("child","/tmp/iter17/child.json")
}
}
