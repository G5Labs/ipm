Include %IPM.Common
Class Iter25.Discovery
{
ClassMethod Run()
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT Name,FormalSpec,ReturnType,ClassMethod FROM %Dictionary.CompiledMethod WHERE Parent = ? AND Name IN ('ExternalName','ExtName','%OnNew') ORDER BY Name","%Studio.SourceControl.ISC")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 while rs.%Next() {
  set n=rs.%Get("Name")
  if 1 {
   write !,"SDK-METHOD|",n,"|",rs.%Get("FormalSpec"),"|",rs.%Get("ReturnType"),"|",rs.%Get("ClassMethod"),!
  }
 }
 write !,"OBS|discovery-complete|1",!
}
}
