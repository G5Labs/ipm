Include %IPM.Common
Class Iter27.Purge
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd)
{
 write !,"BEGIN|",k,"|",cmd,!
 set sc=##class(%IPM.Main).Shell(cmd)
 write !,"END|",k,!
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"COMMAND-ERROR|",k,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod Read(label,kind)
{
 set table=$select(kind="plain":"Iter27Plain",1:"Iter27Hook")_".RowRecord"
 set rs=##class(%SQL.Statement).%ExecDirect(,"select count(*) as N, coalesce(max(Val),0) as V from "_table_" where Token = ?","sentinel")
 do ..Obs(label_"-sql-ok",rs.%SQLCODE=0)
 if rs.%Next() { do ..Obs(label_"-count",rs.%Get("N")),..Obs(label_"-value",rs.%Get("V")) }
 else {write !,"SQL-ERROR|",label,"|",rs.%SQLCODE,"|",rs.%Message,!}
}
ClassMethod Seed(label,kind)
{
 set cls=$select(kind="plain":"Iter27Plain",1:"Iter27Hook")_".Row"
 set sc=$classmethod(cls,"%DeleteExtent")
 do ..Obs(label_"-reset",$$$ISOK(sc))
 set obj=$classmethod(cls,"%New"),obj.Token="sentinel",obj.Val=271
 set sc=obj.%Save()
 do ..Obs(label_"-seed",$$$ISOK(sc))
}
ClassMethod Case(kind,suffix,flags)
{
 set label=kind_"-"_suffix,name="iter27-"_kind,cls=$select(kind="plain":"Iter27Plain",1:"Iter27Hook")_".Row"
 do ..Cmd(label_"-load","load /tmp/iter27/"_kind_" -nodev")
 do ..Seed(label,kind),..Read(label_"-before",kind)
 kill ^||Iter27Hook
 do ..Cmd(label_"-uninstall","uninstall "_name_flags)
 do ..Obs(label_"-module-gone",'##class(%IPM.Storage.Module).NameExists(name))
 do ..Obs(label_"-class-gone",'##class(%Dictionary.CompiledClass).%ExistsId(cls))
 if kind="hook" {
  do ..Obs(label_"-hook-called",+$get(^||Iter27Hook("called")))
  do ..Obs(label_"-hook-purge",+$get(^||Iter27Hook("purge")))
  do ..Obs(label_"-hook-top",+$get(^||Iter27Hook("top")))
  do ..Obs(label_"-hook-nested",+$get(^||Iter27Hook("nested")))
  do ..Obs(label_"-hook-after",+$get(^||Iter27Hook("after")))
 }
 do ..Cmd(label_"-reload","load /tmp/iter27/"_kind_" -nodev")
 do ..Read(label_"-after",kind)
 do ..Obs(label_"-marker",$classmethod(cls,"Marker"))
}
ClassMethod Run()
{
 do ..Case("plain","default",""),..Case("plain","purge"," -purge"),..Case("plain","config"," -DConfigure.Purge=1")
 do ..Case("hook","default",""),..Case("hook","purge"," -purge"),..Case("hook","config"," -DConfigure.Purge=1")
 do ..Seed("calibration","plain"),..Read("calibration-before","plain")
 set sc=##class(Iter27Plain.Row).%DeleteExtent()
 do ..Obs("calibration-delete",$$$ISOK(sc)),..Read("calibration-after","plain")
 do ..Obs("calibration-class-present",##class(%Dictionary.CompiledClass).%ExistsId("Iter27Plain.Row"))
 do ..Obs("workflow-complete",1),..Obs("final-tlevel",$tlevel)
}
}
