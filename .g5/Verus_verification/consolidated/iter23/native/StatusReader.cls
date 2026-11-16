Include (%IPM.Common, %IPM.Formatting)
Class Iter23.StatusReader
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(label,text)
{
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(text)
 write !,"END|",label,!
 do ..Obs(label,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"ERROR-TEXT|",label,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod Watermark()
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT TOP 1 ID FROM %IPM_General.History ORDER BY ID DESC")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 if rs.%Next() {return +rs.%Get("ID")}
 return 0
}
ClassMethod Hex(label,kind,value)
{
 write !,"HEX|",label,"|",kind,"|"
 for i=1:1:$length(value) {set h=$zhex($ascii(value,i)) write $select($length(h)=1:"0",1:"")_h}
 write !
}
ClassMethod Capture(label,command,name)
{
 set before=..Watermark()
 do ..Cmd("install-"_label,command)
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT ID,Action,Package,Success,TimeEnd,Finalized,CommandString FROM %IPM_General.History WHERE ID > ? AND Action=? AND Package=? ORDER BY ID",before,"install",name)
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set count=0,id=""
 while rs.%Next() {
  set count=count+1,id=rs.%Get("ID"),status=rs.%Get("Success"),^||Iter23(label)=id
  do ..Obs(label_"-color-on",$$$ColorScheme)
  do ..Obs(label_"-color-class-present",##class(%Dictionary.CompiledClass).%ExistsId("%IPM.Repo.UniversalSettings"))
  set colorValue=##class(%IPM.Repo.UniversalSettings).GetValue("ColorScheme")
  do ..Obs(label_"-color-value-none",colorValue="none"),..Hex(label,"color-value",colorValue)
  do ..Obs(label_"-record-is-ok",$$$ISOK(status)),..Obs(label_"-record-ended",rs.%Get("TimeEnd")'=""),..Obs(label_"-record-finalized",+rs.%Get("Finalized")),..Obs(label_"-record-command",rs.%Get("CommandString")=command)
  do ..Obs(label_"-raw-length",$length(status))
  do ..Hex(label,"raw",status),..Hex(label,"decoded",$system.Status.GetErrorText(status))
  write !,"ROWID|",label,"|",id,!
 }
 do ..Obs(label_"-record-count",count)
 if count=1 {
  do ..Cmd("summary-"_label,"history find -DID="_id)
  do ..Cmd("detail-"_label,"history details "_id)
 }
}
ClassMethod Run()
{
 do ..Cmd("repo-clear","repo -delete-all")
 do ..Cmd("repo-create","repo -fs -name alpha -depth 1 -path /tmp/iter23/repo")
 do ..Capture("good","install alpha/iter23-status 1.0.0","iter23-status")
 do ..Obs("good-code",$system.CLS.IsMthd("Iter23Status.Value","Who")>0)
 do ..Obs("good-payload",$classmethod("Iter23Status.Value","Who"))
 do ..Capture("controls","install alpha/iter23-controls 1.0.0","iter23-controls")
 do ..Cmd("repo-delete","repo -delete-all")
 do ..Capture("bad","install iter23-missing 1.0.0","iter23-missing")
 do ..Cmd("color-none","config set ColorScheme none")
 do ..Obs("color-none-is-off",'$$$ColorScheme)
 do ..Obs("none-color-class-present",##class(%Dictionary.CompiledClass).%ExistsId("%IPM.Repo.UniversalSettings")),..Obs("none-color-value-none",##class(%IPM.Repo.UniversalSettings).GetValue("ColorScheme")="none")
 for label="good","controls","bad" {
  do ..Cmd("summary-"_label_"-none","history find -DID="_^||Iter23(label))
  do ..Cmd("detail-"_label_"-none","history details "_^||Iter23(label))
 }
 do ..Obs("final-tlevel",$tlevel)
}
}
