/// Test-only observer for iteration35. Not IPM code.
Class IPMCont.ListReader
{

Parameter ROOT = "/tmp/iter35/mods/";

Parameter NAMES = "zpm,i35a,i35b,i35c,i35d,i35e,i35x,i35y,i35pa,i35pb,i35n";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Clean(pLine As %String) As %String
{
    set m = ##class(%Regex.Matcher).%New("\x1B\[[0-9;]*m",pLine)
    quit $zstrip(m.ReplaceAll(""),"*C")
}

ClassMethod Run(pCase As %String, pCommand As %String, ByRef pLines) As %Boolean
{
    kill pLines
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.raw)
    set n = 0
    for i=1:1:$get(raw) {
        set line = ..Clean($get(raw(i)))
        continue:$zstrip(line,"<>W")=""
        set n = n + 1, pLines(n) = line
        write !,"OUT|",pCase,"|",line
    }
    set pLines = n
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

/// Map a printed label to an internal name: "Zeta35" is i35n's external label.
ClassMethod Key(pWord As %String) As %String
{
    quit $select(pWord="Zeta35":"i35n",pWord="IPM":"zpm",1:pWord)
}

ClassMethod VersionCode(pVersion As %String) As %Integer
{
    quit $case(pVersion,"1.0.0":1,"1.10.0":2,"1.9.0":3,"1.0.0-rc.2":4,"1.0.0-rc.10":5,"[missing]":9,:$select(pVersion="":0,1:8))
}

ClassMethod Flat(pCase As %String, pCommand As %String)
{
    do ..Obs(pCase_".ok",..Run(pCase,pCommand,.lines))
    set pos = 0
    for i=1:1:lines {
        set line = $zstrip(lines(i),"<>W")
        set word = $piece(line," ",1)
        set key = ..Key(word)
        continue:(","_..#NAMES_",")'[(","_key_",")
        continue:$data(seen(key))
        set seen(key) = 1, pos = pos + 1
        set rest = $zstrip($piece(line," ",2,*),"<>W")
        if (key = "i35n") || (key = "zpm") { set rest = $zstrip($piece(rest,")",2,*),"<>W") }
        set version = $piece(rest," ",1)
        do ..Obs(pCase_".pos."_key,pos)
        do ..Obs(pCase_".version."_key,..VersionCode(version))
        do ..Obs(pCase_".dev."_key,line["(DeveloperMode)")
        do ..Obs(pCase_".root."_key,line[..#ROOT)
        do ..Obs(pCase_".label."_key,(key="i35n")&&(line["Zeta35 (i35n)"))
    }
    for k=1:1:$length(..#NAMES,",") { set key = $piece(..#NAMES,",",k) do:'$data(seen(key)) ..Obs(pCase_".pos."_key,0) }
    do ..Obs(pCase_".rows",pos)
}

ClassMethod Tree(pCase As %String, pCommand As %String)
{
    do ..Obs(pCase_".ok",..Run(pCase,pCommand,.lines))
    set row = 0
    for i=1:1:lines {
        set line = lines(i)
        set start = 0
        for j=1:1:$length(line) { if $extract(line,j)?1AN { set start = j quit } }
        continue:start=0
        set text = $extract(line,start,*)
        set key = $piece(text," ",1)
        continue:(","_..#NAMES_",")'[(","_key_",")
        set row = row + 1
        set count(key) = $get(count(key)) + 1
        if count(key) = 1 {
            do ..Obs(pCase_".pos."_key,row)
            do ..Obs(pCase_".depth."_key,start\3)
            do ..Obs(pCase_".version."_key,..VersionCode($piece(text," ",2)))
        } else {
            do ..Obs(pCase_".depth2."_key,start\3)
        }
    }
    for k=1:1:$length(..#NAMES,",") { set key = $piece(..#NAMES,",",k) do ..Obs(pCase_".count."_key,$get(count(key))) }
    do ..Obs(pCase_".rows",row)
}

ClassMethod All()
{
    set r = ..#ROOT, ok = 1
    for cmd="load "_r_"i35a","load "_r_"i35b","load "_r_"i35c","load "_r_"i35d","load "_r_"i35e -dev","load "_r_"i35x","load "_r_"i35y","load "_r_"i35pa","load "_r_"i35pb","load "_r_"i35n" {
        set ok = ok && ..Run("setup",cmd,.lines)
    }
    do ..Obs("setup.ok",ok)
    do ..Flat("l0","list-installed")
    do ..Flat("l1","list-installed -s v")
    do ..Flat("l2","list-installed -s n-d")
    do ..Flat("l3","list-installed i35b*")
    do ..Flat("l4","list-installed -showsource")
    do ..Tree("t0","list-installed -tree")
    do ..Tree("t1","list-installed -tree -s v")
    do ..Obs("cut.ok",..Run("cut","uninstall i35a -force",.lines))
    do ..Tree("t2","list-installed -tree")
    do ..Flat("l5","list-installed")
    write !,"NATIVE-END",!
}

}
