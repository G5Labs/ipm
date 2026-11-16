/// Test-only observer for iteration46. Not IPM code.
Class IPMCont.ShowReader
{

Parameter ROOT = "/tmp/iter46/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Has(ByRef pLines, pText As %String) As %Integer
{
    for i=1:1:$get(pLines) { return:pLines(i)[pText 1 }
    quit 0
}

ClassMethod Exact(ByRef pLines, pText As %String) As %Integer
{
    for i=1:1:$get(pLines) { return:$zstrip(pLines(i),"<>W")=pText 1 }
    quit 0
}

ClassMethod Show(pCase As %String, pModule As %String)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell("show "_pModule)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.raw)
    set n = 0, blanks = 0
    for i=1:1:$get(raw) {
        set line = $zstrip($get(raw(i)),"*C")
        write !,"OUT|",pCase,"|",line
        set n = n + 1, lines(n) = line
        set:(line="")&&(n>1) blanks = blanks + 1
    }
    set lines = n
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".desc.folded",..Exact(.lines,"Description:    First line second line"))
    do ..Obs(pCase_".desc.any",..Has(.lines,"Description:"))
    do ..Obs(pCase_".keywords",..Has(.lines,"alpha"))
    do ..Obs(pCase_".after.heading",..Exact(.lines,"After install:"))
    do ..Obs(pCase_".after.l1",..Exact(.lines,"Use it well."))
    do ..Obs(pCase_".after.l2",..Exact(.lines,"Second note."))
    do ..Obs(pCase_".defaults.heading",..Exact(.lines,"Defaults"))
    do ..Obs(pCase_".defaults.greeting",..Exact(.lines,"Greeting = hi"))
    do ..Obs(pCase_".defaults.mode",..Has(.lines,"Mode = p"))
    do ..Obs(pCase_".code.heading",..Exact(.lines,"Code"))
    do ..Obs(pCase_".code.pkg",..Exact(.lines,$zconvert(pModule,"U")_".PKG"))
    do ..Obs(pCase_".code.inc",..Exact(.lines,"S46AInc.INC"))
    do ..Obs(pCase_".code.scoped",..Has(.lines,"S46T.PKG"))
    do ..Obs(pCase_".files.heading",..Exact(.lines,"Files"))
    do ..Obs(pCase_".files.target",..Has(.lines,"-> "))
    do ..Obs(pCase_".depends.heading",..Has(.lines,"Depends on (1)"))
    do ..Obs(pCase_".depends.row",..Has(.lines,"installed 1.0.0"))
    do ..Obs(pCase_".requiredby.heading",..Has(.lines,"Required by directly (1)"))
    do ..Obs(pCase_".requiredby.row",..Exact(.lines,"s46a 1.0.0"))
    do ..Obs(pCase_".sysreq.heading",..Exact(.lines,"System requirements"))
    do ..Obs(pCase_".sysreq.iris",..Exact(.lines,"IRIS >=2020.1"))
    do ..Obs(pCase_".sysreq.interop",..Exact(.lines,"Interoperability disabled"))
    do ..Obs(pCase_".sysreq.health",..Has(.lines,"Health"))
    do ..Obs(pCase_".blocks",blanks)
    set mod = ##class(%IPM.Storage.Module).NameOpen(pModule)
    set req = $select($isobject(mod):mod.SystemRequirements,1:"")
    do ..Obs(pCase_".stored.health.false",$select($isobject(req):(req.Health'="")&&('req.Health),1:0))
    do ..Obs(pCase_".stored.keywords",$select($isobject(mod):(mod.Keywords'=""),1:0))
    set params = 0
    if $isobject(mod) { for i=1:1:mod.Defaults.Count() { set:mod.Defaults.GetAt(i).%IsA("%IPM.Storage.ModuleSetting.Parameter") params = params + 1 } }
    do ..Obs(pCase_".stored.parameters",params)
}

ClassMethod All()
{
    set ok = 1
    for m="s46b","s46a","s46d","s46h" {
        set sc = ##class(%IPM.Main).Shell("load "_..#ROOT_m)
        if 'sc { write !,"ERR|setup-",m,"|",$system.Status.GetErrorText(sc),! }
        set ok = ok && sc
    }
    do ..Obs("setup.ok",ok)
    do ..Show("a","s46a")
    do ..Show("b","s46b")
    do ..Show("d","s46d")
    do ..Show("h","s46h")
    write !,"NATIVE-END",!
}

}
