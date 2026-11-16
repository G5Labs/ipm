Include %IPM.Common
Class Iter16.Provenance
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Root(k) { return "/tmp/iter16/repos/"_k_"/" }
ClassMethod Cmd(k,cmd) As %Boolean
{
 write !,"BEGIN|",k,!
 set sc=##class(%IPM.Main).Shell(cmd)
 write !,"END|",k,!
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {set ^Iter16("errors",k)=$system.Status.GetErrorText(sc) write !,"RAW|",k,"|",^Iter16("errors",k),!}
 return $$$ISOK(sc)
}
ClassMethod Config(label,name,root)
{
 set d=##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen(name)
 do ..Obs(label_"-exists",$isobject(d))
 if '$isobject(d) {return}
 do ..Obs(label_"-root",d.Root=..Root(root)),..Obs(label_"-depth",d.Depth),..Obs(label_"-details-shape",d.Details=(..Root(root)_"*/module.xml"))
 set ^Iter16("config",root,"details")=d.Details
 write !,"RAW-CONFIG|",label,"|",d.Root,"|",d.Details,!
}
ClassMethod State(label,supplier,root,payload)
{
 set m=##class(%IPM.Storage.Module).NameOpen("iter16-origin")
 do ..Obs(label_"-module",$isobject(m))
 if $isobject(m) {
  do ..Obs(label_"-supplier",m.Repository=supplier),..Obs(label_"-supplier-empty",m.Repository="")
  do ..Obs(label_"-version",m.VersionString="1.0.0")
  do ..Obs(label_"-root",##class(%File).NormalizeDirectory(m.Root)=##class(%File).NormalizeDirectory(..Root(root)_"iter16-origin/"))
 }
 set available=$system.CLS.IsMthd("Iter16Payload.Value","Who")
 do ..Obs(label_"-code",available)
 if available {do ..Obs(label_"-payload",$classmethod("Iter16Payload.Value","Who"))}
 do ..Cmd("show-"_label,"show iter16-origin")
}
ClassMethod Watermark()
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT TOP 1 ID FROM %IPM_General.History ORDER BY ID DESC")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 if rs.%Next() {return +rs.%Get("ID")}
 return 0
}
ClassMethod Capture(label,action,cmd,supplier,root)
{
 set before=..Watermark()
 do ..Cmd(label,cmd)
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT ID,Action,Package,Version_Major,Version_Minor,Version_Patch,Version_Prerelease,Version_Build,SourceName,SourceMoniker,SourceDetails,TimeEnd,Success,Committed,CommandString,Finalized FROM %IPM_General.History WHERE ID > ? AND Action = ? AND Package = ? AND CommandString = ? ORDER BY ID",before,action,"iter16-origin",cmd)
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set count=0
 while rs.%Next() {
  set count=count+1,id=rs.%Get("ID"),^Iter16("history",label)=id
  do ..Obs(label_"-history-id-positive",id>before)
  do ..Obs(label_"-history-success",$$$ISOK(rs.%Get("Success"))),..Obs(label_"-history-finalized",+rs.%Get("Finalized")),..Obs(label_"-history-ended",rs.%Get("TimeEnd")'="")
  do ..Obs(label_"-history-committed",+rs.%Get("Committed"))
  do ..Obs(label_"-history-version",(rs.%Get("Version_Major")=1)&&(rs.%Get("Version_Minor")=0)&&(rs.%Get("Version_Patch")=0)&&(rs.%Get("Version_Prerelease")="")&&(rs.%Get("Version_Build")=""))
  if supplier'="" {
   do ..Obs(label_"-history-name",rs.%Get("SourceName")=supplier),..Obs(label_"-history-type",rs.%Get("SourceMoniker")="filesystem")
   do ..Obs(label_"-history-details",rs.%Get("SourceDetails")=$get(^Iter16("config",root,"details")))
  } else {
   do ..Obs(label_"-history-name-empty",rs.%Get("SourceName")=""),..Obs(label_"-history-type-empty",rs.%Get("SourceMoniker")=""),..Obs(label_"-history-details-empty",rs.%Get("SourceDetails")="")
  }
  write !,"RAW-HISTORY|",label,"|",id,"|",rs.%Get("SourceName"),"|",rs.%Get("SourceMoniker"),"|",rs.%Get("SourceDetails"),!
 }
 do ..Obs(label_"-history-count",count)
 if count=1 {do ..Cmd("details-"_label,"history details "_^Iter16("history",label))}
}
ClassMethod ReadOld(label,original,supplier,root)
{
 set id=$get(^Iter16("history",original))
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT SourceName,SourceMoniker,SourceDetails FROM %IPM_General.History WHERE ID = ?",id)
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set found=rs.%Next()
 do ..Obs(label_"-row",found)
 if found {
  do ..Obs(label_"-name",rs.%Get("SourceName")=supplier),..Obs(label_"-type",rs.%Get("SourceMoniker")="filesystem")
  do ..Obs(label_"-details",rs.%Get("SourceDetails")=$get(^Iter16("config",root,"details")))
 }
 do ..Cmd("details-"_label,"history details "_id)
}
ClassMethod Run()
{
 kill ^Iter16
 do ..Cmd("repo-clear","repo -delete-all")
 do ..Cmd("missing","install missing/iter16-origin")
 do ..Obs("missing-no-module",'##class(%IPM.Storage.Module).NameExists("iter16-origin")),..Obs("missing-no-code",'$system.CLS.IsMthd("Iter16Payload.Value","Who"))
 do ..Cmd("repo-alpha","repo -fs -name alpha -depth 1 -path "_..Root("a")),..Cmd("repo-beta","repo -fs -name beta -depth 1 -path "_..Root("b"))
 do ..Config("config-a","alpha","a"),..Config("config-b","beta","b")
 do ..Capture("a","install","install alpha/iter16-origin","alpha","a"),..State("a","alpha","a",1)
 do ..Cmd("delete-alpha","repo -delete -name alpha")
 do ..ReadOld("after-delete","a","alpha","a"),..State("after-delete","alpha","a",1)
 do ..Cmd("recreate-alpha","repo -fs -name alpha -depth 1 -path "_..Root("c"))
 do ..Config("config-c","alpha","c")
 do ..ReadOld("after-recreate","a","alpha","a"),..State("after-recreate","alpha","a",1)
 do ..Cmd("uninstall-a","uninstall iter16-origin")
 do ..Capture("c","install","install alpha/iter16-origin","alpha","c"),..State("c","alpha","c",3)
 do ..ReadOld("old-a-after-c","a","alpha","a")
 do ..Cmd("uninstall-c","uninstall iter16-origin")
 do ..Capture("b","install","install beta/iter16-origin","beta","b"),..State("b","beta","b",2)
 do ..Capture("local","load","load -nodev "_..Root("a")_"iter16-origin","","a"),..State("local","","a",1)
 do ..ReadOld("old-b-after-local","b","beta","b"),..ReadOld("old-a-final","a","alpha","a")
 do ..Cmd("history-find","history find -DPackage=iter16-origin")
 do ..Obs("final-tlevel",$tlevel)
}
}
