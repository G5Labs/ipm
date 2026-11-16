Class Iter27.MappingPlacement
{
ClassMethod Run()
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"select Name, ClassMethod, FormalSpec, ReturnType, Origin from %Dictionary.CompiledMethod where Parent = '%Studio.Project' order by Name")
 while rs.%Next() {
  set n=rs.%Get("Name")
  if (n["New")!(n["Create")!(n["Add")!(n["Insert")!(n["Save")!(n["Export")!(n["Deploy")!(n["Install") {
   write !,"SDKMETHOD|",n,"|",rs.%Get("ClassMethod"),"|",rs.%Get("FormalSpec"),"|",rs.%Get("ReturnType"),"|",rs.%Get("Origin"),!
  }
 }
 write !,"OBS|sdk-metadata-complete|1",!
}
}
