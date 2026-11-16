/// Test-only observer for iteration52. Not IPM code.
Class IPMCont.StoreReader
{

Parameter ROOT = "/tmp/iter52/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod LifeCode(pValue As %String) As %Integer
{
    quit $case(pValue,"":0,"Module":1,"L52.Life":2,"NoSuch52.Life":3,"L52P.Life":5,"%IPM.Lifecycle.Module":4,:9)
}

ClassMethod Shell(pCase As %String, pCommand As %String) As %Boolean
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    for i=1:1:$get(lines) { write !,"OUT|",pCase,"|",$zstrip($get(lines(i)),"*C") }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

/// Stored record fields read by SQL so no object hook runs.
ClassMethod Row(pName As %String, Output pMod, Output pUpd, Output pLife) As %Boolean
{
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select LastModifiedTimestamp,LastUpdated,LifecycleClass from %IPM_Storage.ModuleItem where Name = ?",pName)
    quit:'rs.%Next() 0
    set pMod = rs.%Get("LastModifiedTimestamp"), pUpd = rs.%Get("LastUpdated"), pLife = rs.%Get("LifecycleClass")
    quit 1
}

ClassMethod M52(pCase As %String)
{
    set mod = ##class(%IPM.Storage.Module).NameOpen("m52")
    do ..Obs(pCase_".resources",$select($isobject(mod):mod.Resources.Count(),1:-1))
    do ..Obs(pCase_".deps",$select($isobject(mod):mod.Dependencies.Count(),1:-1))
    do ..Obs(pCase_".r2defined",##class(%Dictionary.ClassDefinition).%ExistsId("M52.R2"))
    do ..Obs(pCase_".dep52installed",##class(%IPM.Storage.Module).NameExists("dep52"))
    do ..Obs(pCase_".greeting",$case($get(^IPMCONT52("greeting")),"a":1,"b":2,:0))
}

ClassMethod All()
{
    set r = ..#ROOT
    kill ^IPMCONT52
    do ..Obs("c0.ok",..Shell("c0","load "_r_"dep52") && ..Shell("c0","load "_r_"m52"))
    do ..M52("c0")
    do ..Row("m52",.mod0,.upd0)
    hang 1.2
    do ..Obs("c1.ok",..Shell("c1","load "_r_"m52"))
    do ..Row("m52",.mod1,.upd1)
    do ..Obs("c1.modchanged",mod1'=mod0)
    do ..Obs("c1.updchanged",upd1'=upd0)
    do ..M52("c1")
    hang 1.2
    do ..Obs("c2.ok",..Shell("c2","load "_r_"m52v2"))
    do ..Row("m52",.mod2,.upd2)
    do ..Obs("c2.modchanged",mod2'=mod1)
    do ..Obs("c2.updchanged",upd2'=upd1)
    do ..M52("c2")
    do ..Obs("l1.ok",..Shell("l1","load "_r_"l52"))
    do ..Row("l52",,,.life1)
    do ..Obs("l1.life",..LifeCode(life1))
    do ..Obs("l1.custom",$get(^IPMCONT52("life")))
    do ..Obs("l2.ok",..Shell("l2","load "_r_"l52"))
    do ..Row("l52",,,.life2)
    do ..Obs("l2.life",..LifeCode(life2))
    do ..Obs("l2.custom",$get(^IPMCONT52("life")))
    do ..Obs("p1.ok",..Shell("p1","load "_r_"l52p"))
    do ..Row("l52p",,,.lifep)
    do ..Obs("p1.life",..LifeCode(lifep))
    do ..Obs("p1.custom",$get(^IPMCONT52("lifep")))
    do ..Obs("n1.ok",..Shell("n1","load "_r_"n52"))
    do ..Row("n52",,,.life3)
    do ..Obs("n1.life",..LifeCode(life3))
    do ..Obs("o1.ok",..Shell("o1","load "_r_"o52"))
    do ..Obs("o1.installed",##class(%IPM.Storage.Module).NameExists("o52"))
    write !,"NATIVE-END",!
}

}
