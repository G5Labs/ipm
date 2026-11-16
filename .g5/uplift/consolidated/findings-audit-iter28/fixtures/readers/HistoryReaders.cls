Class FindingsAudit.HistoryReaders
{
ClassMethod Run()
{
 set sc=##class(%IPM.Main).Shell("history delete -confirm")
 set h=##class(%IPM.General.History).LoadInit("audit-user") do h.Finalize(1) kill h
 set sc=##class(%IPM.Test.Utils).CreateNamespace("AUDITREADER")
 write !,"API|namespace|",$system.Status.IsOK(sc),!
 new $namespace
 set $namespace="AUDITREADER"
 set h=##class(%IPM.General.History).LoadInit("audit-other") do h.Finalize(1) kill h
 set $namespace="USER"
 for query="Records","GlobalRecords" {
  set r=##class(%ResultSet).%New("%IPM.General.History:"_query)
  set sc=r.Execute(0,0),u=0,o=0
  while r.Next() { set pkg=r.Get("Package") set u=u+(pkg="audit-user"),o=o+(pkg="audit-other") }
  write !,"API|",query,"|status|",$system.Status.IsOK(sc),!
  write "API|",query,"|user|",u,"|other|",o,!
  if query="GlobalRecords" {write "API|global-repeated|",(u>1),!}
 }
 set before=+$get(^IPM.General.HistoryD)
 for cmd="update audit-missing","reinstall audit-missing","uninstall" {
  set sc=##class(%IPM.Main).Shell(cmd)
  write !,"REFUSAL|",cmd,"|ok|",$system.Status.IsOK(sc),!
 }
 write "REFUSAL|allocated|",(+$get(^IPM.General.HistoryD)-before),!
 set sc=##class(%IPM.Main).Shell("default-modifiers -set -DNoTransaction=1")
 do ..Capture("default-visible")
 set sc=##class(%IPM.Main).Shell("default-modifiers -delete")
 do ..Capture("plain-visible")
 set sc=##class(%IPM.Main).Shell("history delete -globally -DSucess=0")
 write !,"GLOBALDELETE|bad-ok|",$system.Status.IsOK(sc),!
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT COUNT(*) AS N FROM %IPM_General.History WHERE Package='audit-user'") do rs.%Next()
 write "GLOBALDELETE|sentinel-afterbad|",rs.N,!
 set sc=##class(%IPM.Main).Shell("history delete -globally -DPackage=audit-user")
 write "GLOBALDELETE|good-ok|",$system.Status.IsOK(sc),!
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT COUNT(*) AS N FROM %IPM_General.History WHERE Package='audit-user'") do rs.%Next()
 write "GLOBALDELETE|sentinel-aftergood|",rs.N,!

}
ClassMethod Capture(label)
{
 set io=$io
 open "/tmp/audit-history-capture.txt":("WNS"):5
 use "/tmp/audit-history-capture.txt"
 set sc=##class(%IPM.Main).Shell("history find")
 close "/tmp/audit-history-capture.txt"
 use io
 set f=##class(%Stream.FileCharacter).%New() do f.LinkToFile("/tmp/audit-history-capture.txt")
 set txt=f.Read(32000)
 write !,"FILTER|",label,"|",(txt["audit-user"),!
 write "FILTER|",label,"-ok|",$system.Status.IsOK(sc),!
}
}
