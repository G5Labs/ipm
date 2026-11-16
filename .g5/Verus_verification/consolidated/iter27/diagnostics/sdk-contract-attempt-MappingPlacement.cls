Class Iter27.MappingPlacement
{
ClassMethod Run()
{
 set c=##class(%Dictionary.ClassDefinition).%OpenId("%Studio.Project")
 for i=1:1:4 {
  set n=$listget($listbuild("DeployToFile","DeployToStream","AddItem","%OnNew"),i)
  set m=c.Methods.GetAt(n)
  write !,"SDK-CONTRACT-BEGIN|",n,!,m.Description,!
  set stream=m.Implementation
  if $isobject(stream) { do stream.Rewind() while 'stream.AtEnd { write stream.Read(4000) } }
  write !,"SDK-CONTRACT-END|",n,!
 }
 write !,"OBS|sdk-contract-complete|1",!
}
}
