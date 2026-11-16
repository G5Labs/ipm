Class FindingsAudit.Read
{
ClassMethod Run()
{
 for case="dollar","neighbor","dotted","cycle","build" {
  write !,"BEGINCASE|",case,!
  set sc=##class(%IPM.Main).Shell("load /tmp/uplift/params/"_case)
  write !,"STATUS|",case,"|",$system.Status.IsOK(sc),!
  set m=##class(%IPM.Storage.Module).NameOpen("audit-"_case)
  write "VERSION|",case,"|",$select($isobject(m):m.VersionString,1:"absent"),!
  write "ENDCASE|",case,!
 }
 for case="dollar-env","neighbor-env" {
  set sc=##class(%IPM.Main).Shell("load /tmp/uplift/params/env -env /tmp/uplift/params/"_case_".json")
  write !,"ENVSTATUS|",case,"|",$system.Status.IsOK(sc),!
  write "ENVREADER|",case,"|",$get(^FindingsAuditEnv),!
 }
 try {set sc=$zpm("version") write !,"ZPMFUNCTION|returned|",$system.Status.IsOK(sc),!} catch ex {write !,"ZPMFUNCTION|error|",ex.Name,!}
 set sc=##class(%IPM.Main).Shell("version") write !,"ZPMSHELL|returned|",$system.Status.IsOK(sc),!
}
ClassMethod ReadEnv() As %Status
{
 set ^FindingsAuditEnv=##class(%IPM.General.EnvironmentConfig).%Get().GetArg("audit-env","price")
 return 1
}
}
