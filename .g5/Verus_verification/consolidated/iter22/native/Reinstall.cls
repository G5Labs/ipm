Include %IPM.Common
Class Iter22.Reinstall
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Root(k) { return "/tmp/iter16/repos/"_k_"/" }
ClassMethod Cmd(k,cmd) As %Boolean
{
 write !,"BEGIN|",k,!
 set sc=##class(%IPM.Main).Shell(cmd)
 write !,"END|",k,!
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {set ^||Iter22("errors",k)=$system.Status.GetErrorText(sc) write !,"RAW|",k,"|",^||Iter22("errors",k),!}
 return $$$ISOK(sc)
}
ClassMethod Config(label,name,root)
{
 set d=##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen(name)
 do ..Obs(label_"-exists",$isobject(d))
 if '$isobject(d) {return}
 do ..Obs(label_"-root",d.Root=..Root(root)),..Obs(label_"-depth",d.Depth),..Obs(label_"-details-shape",d.Details=(..Root(root)_"*/module.xml"))
 set ^||Iter22("config",root,"details")=d.Details
 write !,"RAW-CONFIG|",label,"|",d.Root,"|",d.Details,!
}
ClassMethod State(label,supplier,root,payload)
{
 set m=##class(%IPM.Storage.Module).NameOpen("iter16-origin")
 do ..Obs(label_"-module",$isobject(m))
 if $isobject(m) {
  do ..Obs(label_"-supplier",m.Repository=supplier),..Obs(label_"-supplier-empty",m.Repository="")
  do ..Obs(label_"-current-alpha",m.Repository="alpha"),..Obs(label_"-current-beta",m.Repository="beta")
  do ..Obs(label_"-root-exists",##class(%File).DirectoryExists(m.Root)),..Obs(label_"-developer-mode",m.DeveloperMode)
  do ..Obs(label_"-version",m.VersionString="1.0.0")
  do ..Obs(label_"-root",##class(%File).NormalizeDirectory(m.Root)=##class(%File).NormalizeDirectory(..Root(root)_"iter16-origin/"))
 }
 set available=$system.CLS.IsMthd("Iter16Payload.Value","Who")>0
 do ..Obs(label_"-code",available)
 if available {do ..Obs(label_"-payload",$classmethod("Iter16Payload.Value","Who"))}
 do ..Cmd("show-"_label,"show iter16-origin")
 do ..Cmd("list-alpha-"_label,"list -repository alpha"),..Cmd("list-beta-"_label,"list -repository beta")
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
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT ID,Action,Package,Version_Major,Version_Minor,Version_Patch,Version_Prerelease,Version_Build,SourceName,SourceMoniker,SourceDetails,TimeEnd,Success,Committed,CommandString,Finalized FROM %IPM_General.History WHERE ID > ? AND Action = ? AND Package = ? ORDER BY ID",before,action,"iter16-origin")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set count=0
 while rs.%Next() {
  set count=count+1,id=rs.%Get("ID"),^||Iter22("history",label)=id
  do ..Obs(label_"-history-command",rs.%Get("CommandString")=cmd)
  write !,"COMMAND|",label,"|",rs.%Get("CommandString"),!
  do ..Obs(label_"-history-id-positive",id>before)
  do ..Obs(label_"-history-success",$$$ISOK(rs.%Get("Success"))),..Obs(label_"-history-finalized",+rs.%Get("Finalized")),..Obs(label_"-history-ended",rs.%Get("TimeEnd")'="")
  do ..Obs(label_"-history-committed",+rs.%Get("Committed"))
  do ..Obs(label_"-history-version",(rs.%Get("Version_Major")=1)&&(rs.%Get("Version_Minor")=0)&&(rs.%Get("Version_Patch")=0)&&(rs.%Get("Version_Prerelease")="")&&(rs.%Get("Version_Build")=""))
  if supplier'="" {
   do ..Obs(label_"-history-name",rs.%Get("SourceName")=supplier),..Obs(label_"-history-type",rs.%Get("SourceMoniker")="filesystem")
   do ..Obs(label_"-history-details",rs.%Get("SourceDetails")=$get(^||Iter22("config",root,"details")))
  } else {
   do ..Obs(label_"-history-name-empty",rs.%Get("SourceName")=""),..Obs(label_"-history-type-empty",rs.%Get("SourceMoniker")=""),..Obs(label_"-history-details-empty",rs.%Get("SourceDetails")="")
  }
  write !,"RAW-HISTORY|",label,"|",id,"|",rs.%Get("SourceName"),"|",rs.%Get("SourceMoniker"),"|",rs.%Get("SourceDetails"),!
 }
 do ..Obs(label_"-history-count",count)
 if count=1 {do ..Cmd("details-"_label,"history details "_^||Iter22("history",label))}
}
ClassMethod ReadOld(label,original,supplier,root)
{
 set id=$get(^||Iter22("history",original))
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT SourceName,SourceMoniker,SourceDetails FROM %IPM_General.History WHERE ID = ?",id)
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set found=rs.%Next()
 do ..Obs(label_"-row",found)
 if found {
  do ..Obs(label_"-name",rs.%Get("SourceName")=supplier),..Obs(label_"-type",rs.%Get("SourceMoniker")="filesystem")
  do ..Obs(label_"-details",rs.%Get("SourceDetails")=$get(^||Iter22("config",root,"details")))
 }
 do ..Cmd("details-"_label,"history details "_id)
}
ClassMethod Absent(label)
{
 do ..Obs(label_"-no-module",'##class(%IPM.Storage.Module).NameExists("iter16-origin"))
 do ..Obs(label_"-no-code",$system.CLS.IsMthd("Iter16Payload.Value","Who")=0)
}
ClassMethod Catalog()
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT VersionString FROM %IPM_Repo_Filesystem.Cache WHERE Root=? AND Name=? ORDER BY VersionString","/tmp/iter16/repos/a/","iter16-origin")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set count=0,one=0,two=0
 while rs.%Next() {
  set count=count+1,v=rs.%Get("VersionString")
  if v="1.0.0" {set one=1}
  if v="2.0.0" {set two=1}
 }
 do ..Obs("alpha-cache-count",count),..Obs("alpha-offers-one",one),..Obs("alpha-offers-two",two)
 set a=##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen("alpha"),b=##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen("beta")
 do ..Obs("alpha-created-before-beta",+a.%Id()<+b.%Id())
 do ..Cmd("search-latest","search iter16-origin -versions")
}
ClassMethod HistoryDelta(label,before)
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT ID,Action,Success FROM %IPM_General.History WHERE ID > ? ORDER BY ID",before)
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set count=0,actions=1,failures=1
 while rs.%Next() {
  set count=count+1,actions=actions&&(rs.%Get("Action")="install"),failures=failures&&'$$$ISOK(rs.%Get("Success"))
  set ^||Iter22("failed-history",label)=rs.%Get("ID")
  write !,"FAILED-HISTORY|",label,"|",rs.%Get("ID"),"|",rs.%Get("Action"),"|",$$$ISOK(rs.%Get("Success")),!
 }
 do ..Obs(label_"-new-history-count",count),..Obs(label_"-new-history-install",actions),..Obs(label_"-new-history-failed",failures)
 if count=1 {do ..Cmd("failed-summary-"_label,"history find -DID="_^||Iter22("failed-history",label))}
}
ClassMethod Run()
{
 kill ^||Iter22
 do ..Cmd("repo-clear","repo -delete-all")
 do ..Cmd("repo-alpha","repo -fs -name alpha -depth 1 -path "_..Root("a")),..Cmd("repo-beta","repo -fs -name beta -depth 1 -path "_..Root("b"))
 do ..Config("config-alpha","alpha","a"),..Config("config-beta","beta","b"),..Catalog()
 do ..Capture("initial-beta","install","install beta/iter16-origin 1.0.0","beta","b"),..State("initial","beta","b",2)
 do ..Capture("re-alpha","install","reinstall iter16-origin","alpha","a"),..State("re-alpha","alpha","a",1)
 do ..ReadOld("old-beta-after-alpha","initial-beta","beta","b")
 do ..Capture("reset-beta","install","install beta/iter16-origin 1.0.0","beta","b"),..State("reset","beta","b",2)
 do ..Cmd("disable-alpha","repo -fs -name alpha -disable -depth 1 -path "_..Root("a"))
 set a=##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen("alpha")
 do ..Obs("alpha-disabled",'a.Enabled)
 do ..Capture("re-beta","install","reinstall iter16-origin","beta","b"),..State("re-beta","beta","b",2)
 do ..ReadOld("old-alpha-after-beta","re-alpha","alpha","a")
 do ..Cmd("delete-alpha","repo -name alpha -delete"),..Cmd("delete-beta","repo -name beta -delete")
 do ..Obs("alpha-absent",'##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("alpha")),..Obs("beta-absent",'##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("beta"))
 do ..State("before-refusals","beta","b",2)
 set before=..Watermark()
 do ..Cmd("re-no-repo","reinstall iter16-origin"),..HistoryDelta("no-repo",before),..State("no-repo","beta","b",2)
 set before=..Watermark()
 do ..Cmd("re-dev-no-repo","reinstall iter16-origin -dev"),..HistoryDelta("dev-no-repo",before),..State("dev-no-repo","beta","b",2)
 set before=..Watermark()
 do ..Cmd("re-missing-installed","reinstall iter22-missing")
 do ..Obs("missing-no-new-history",..Watermark()=before)
 do ..State("missing-other","beta","b",2)
 do ..ReadOld("old-beta-after-delete","initial-beta","beta","b"),..ReadOld("old-alpha-after-delete","re-alpha","alpha","a")
 do ..Obs("final-tlevel",$tlevel)
}
}
