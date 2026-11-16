Include %IPM.Common
Class Iter25.Discovery
{
ClassMethod Run()
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT Name,Origin,Description FROM %Dictionary.CompiledMethod WHERE Parent = ? AND Name IN ('ExternalName','ExtName','Name') ORDER BY Name","%Studio.SourceControl.File")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 while rs.%Next() {
  set n=rs.%Get("Name"),origin=rs.%Get("Origin")
  write !,"SDK-ORIGIN|",n,"|",origin,"|",rs.%Get("Description"),!
  set def=##class(%Dictionary.MethodDefinition).%OpenId(origin_"||"_n)
  if $isobject(def) {
   set body=def.Implementation
   write !,"SDK-BODY-BEGIN|",n,!
   if $isobject(body) {while 'body.AtEnd {write body.Read()}} else {write body}
   write !,"SDK-BODY-END|",n,!
  }
 }
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT Parent,Name,FormalSpec FROM %Dictionary.CompiledMethod WHERE Parent LIKE '%Studio%' AND (Name LIKE '%External%' OR Name LIKE '%SourcePath%' OR Name LIKE '%FullSource%') ORDER BY Parent,Name")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 while rs.%Next() {write !,"SDK-CANDIDATE|",rs.%Get("Parent"),"|",rs.%Get("Name"),"|",rs.%Get("FormalSpec"),!}
 write !,"OBS|discovery-complete|1",!
}
}
