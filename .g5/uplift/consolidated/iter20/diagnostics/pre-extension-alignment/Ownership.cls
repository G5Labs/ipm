Include %IPM.Common
Class Iter20.Ownership
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(label,command)
{
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(command)
 write !,"END|",label,!
 do ..Obs(label,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"STATUS|",label,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod Inventory(label)
{
 set rs=##class(%Library.RoutineMgr).StudioOpenDialogFunc("*.CLS",1,1,0,1,,0,,,0)
 do ..Obs(label_"-inventory-sql-ok",rs.%SQLCODE=0)
 set loose=0,named=0
 while rs.%Next(.sc) {
  $$$ThrowOnError(sc)
  set n=rs.%Get("Name")
  if n="Iter20Loose.Checks.CLS" {set loose=1}
  if n="Iter20Named.Loose.CLS" {set named=1}
 }
 do ..Obs(label_"-inventory-loose",loose),..Obs(label_"-inventory-named",named)
}
ClassMethod Item(label,short,cls,expected)
{
 set full=cls_".CLS",id="",ref="",resource=""
 set exists=##class(%IPM.Storage.ResourceReference).NonNullResourceNameExists(full,.id)
 do ..Obs(label_"-"_short_"-reference",exists)
 if exists {
  set r=##class(%IPM.Storage.ResourceReference).%OpenId(id),owner=r.Module
  do ..Obs(label_"-"_short_"-reference-owner",$isobject(owner))
  if $isobject(owner) {do ..Obs(label_"-"_short_"-reference-owner-match",owner.Name=expected)}
 }
 set home=##class(%IPM.ExtensionBase.Utils).FindHomeModule(full,.resource,.ref)
 do ..Obs(label_"-"_short_"-home",$isobject(home))
 set homeName=$select($isobject(home):home.Name,1:"")
 do ..Obs(label_"-"_short_"-home-match",homeName=expected)
 do ..Obs(label_"-"_short_"-home-reference",$isobject(ref))
 do ..Obs(label_"-"_short_"-home-name-reader",##class(%IPM.ExtensionBase.Utils).GetHomeModuleName(full)=expected)
 set available=$system.CLS.IsMthd(cls,"Who")>0
 do ..Obs(label_"-"_short_"-code",available)
 if available {do ..Obs(label_"-"_short_"-payload",$classmethod(cls,"Who"))}
 write !,"HOME|",label,"|",short,"|",homeName,"|",resource,"|",$isobject(ref),!
}
ClassMethod State(label,loose,named)
{
 do ..Inventory(label)
 do ..Item(label,"loose","Iter20Loose.Checks",loose),..Item(label,"named","Iter20Named.Loose",named)
 do ..Cmd("orphans-"_label,"orphans -type CLS")
}
ClassMethod Run()
{
 set sc=$system.OBJ.Load("/tmp/iter20/free/Iter20Loose/Checks.cls","ck")
 do ..Obs("free-loose-compile",$$$ISOK(sc))
 set sc=$system.OBJ.Load("/tmp/iter20/free/Iter20Named/Loose.cls","ck")
 do ..Obs("free-named-compile",$$$ISOK(sc))
 do ..State("free","","")
 do ..Cmd("load-claim","load /tmp/iter20/claim")
 do ..State("claimed","iter20-claim","")
 do ..Cmd("load-naming","load /tmp/iter20/naming")
 set named=##class(%IPM.Storage.Module).NameOpen("iter20named")
 do ..Obs("naming-module",$isobject(named))
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT COUNT(*) AS N FROM %IPM_Storage.ResourceReference WHERE ModuleItem=?",named.%Id())
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 if rs.%Next() {do ..Obs("naming-resource-count",rs.%Get("N"))}
 do ..State("naming","iter20-claim","iter20named")
 do ..Cmd("load-exact","load /tmp/iter20/exact")
 do ..State("exact","iter20-claim","iter20-exact")
 do ..Obs("final-tlevel",$tlevel)
}
}
