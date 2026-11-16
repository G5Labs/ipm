/// Test-only observer for iteration33. Not IPM code.
Class Iter33.Reader
{

Parameter MODULES = "i33a,i33b,i33c,i33d,i33e,i33f,i33g";

ClassMethod Code(pVersion As %String) As %Integer
{
    quit:pVersion="" 0
    quit ($piece(pVersion,".",1)*100) + ($piece(pVersion,".",2)*10) + $piece(pVersion,".",3)
}

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Installed(pName As %String) As %String
{
    quit:'##class(%IPM.Storage.Module).NameExists(pName) ""
    quit ##class(%IPM.Storage.Module).NameOpen(pName).VersionString
}

/// Run a shell command; return captured lines with ANSI sequences and control characters removed.
ClassMethod Exec(pLabel As %String, pCommand As %String, Output pLines) As %Status
{
    kill pLines
    write !,"BEGIN|",pLabel,"|",pCommand
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set pLines = 0
    for i=1:1:$get(lines) {
        set raw = $get(lines(i))
        set clean = ##class(%Regex.Matcher).%New("\x{1B}\[[0-9;]*m",raw).ReplaceAll("")
        set clean = $zstrip(clean,"*C")
        set pLines($increment(pLines)) = clean
        write !,"OUT|",pLabel,"|",clean
    }
    if 'sc { write !,"ERR|",pLabel,"|",$system.Status.GetErrorText(sc) }
    do ..Obs(pLabel_".ok",$system.Status.IsOK(sc))
    quit sc
}

ClassMethod Cmd(pLabel As %String, pCommand As %String)
{
    do ..Exec(pLabel,pCommand,.lines)
    do ..State(pLabel)
    write !,"END|",pLabel
}

ClassMethod State(pLabel As %String)
{
    for i=1:1:$length(..#MODULES,",") {
        set name = $piece(..#MODULES,",",i)
        do ..Obs(pLabel_".inst."_name,..Code(..Installed(name)))
    }
}

/// Flat report: "<name> <version> (installed)" or "<name> <version> @ <repo>"; warnings carry ": <error>".
ClassMethod Flat(pLabel As %String, pCommand As %String)
{
    do ..Exec(pLabel,pCommand,.lines)
    set warn = 0
    for i=1:1:lines { if lines(i)["WARNING: some errors occurred." { set warn = 1 } }
    do ..Obs(pLabel_".warning",warn)
    for m=1:1:$length(..#MODULES,",") {
        set name = $piece(..#MODULES,",",m)
        set listed = 0, version = 0, server = 0, warned = 0
        for i=1:1:lines {
            set line = lines(i)
            continue:$piece(line," ",1)'=name
            if line[": " { set warned = warned + 1 continue }
            set listed = listed + 1, version = ..Code($piece(line," ",2))
            set server = $select(line["(installed)":1,line["@ ":2,1:3)
        }
        do ..Obs(pLabel_".list."_name,listed)
        do ..Obs(pLabel_".ver."_name,version)
        do ..Obs(pLabel_".server."_name,server)
        do ..Obs(pLabel_".warn."_name,warned)
    }
    write !,"END|",pLabel
}

/// Tree report: one "<padding><name> <version>" line per node; record first line index, node count and name column.
ClassMethod Tree(pLabel As %String, pCommand As %String)
{
    do ..Exec(pLabel,pCommand,.lines)
    set nodes = 0
    for i=1:1:lines {
        set m = ##class(%Regex.Matcher).%New("(i33[a-g]) ([0-9]+\.[0-9]+\.[0-9]+)\s*$",lines(i))
        continue:'m.Locate()
        set nodes = nodes + 1, name = m.Group(1)
        set count(name) = $get(count(name)) + 1
        if '$data(first(name)) { set first(name) = nodes, column(name) = m.Start(1), version(name) = ..Code(m.Group(2)) }
    }
    do ..Obs(pLabel_".nodes",nodes)
    for k=1:1:$length(..#MODULES,",") {
        set name = $piece(..#MODULES,",",k)
        do ..Obs(pLabel_".line."_name,$get(first(name)))
        do ..Obs(pLabel_".count."_name,$get(count(name)))
        do ..Obs(pLabel_".col."_name,$get(column(name)))
        do ..Obs(pLabel_".ver."_name,$get(version(name)))
    }
    write !,"END|",pLabel
}

ClassMethod Run()
{
    do ..Exec("repos","repo -list",.x)
    do ..Cmd("repo","repo -fs -name r33 -depth 1 -path /tmp/iter33/repo/")
    do ..Cmd("ld-a2","load /tmp/iter33/loose/i33a-2.0.0/")
    do ..Cmd("ld-b","load /tmp/iter33/loose/i33b-1.0.0/")
    do ..Cmd("ld-c","load /tmp/iter33/loose/i33c-1.0.0/")
    do ..Cmd("ld-d","load /tmp/iter33/loose/i33d-1.0.0/")
    do ..Cmd("ld-e","load /tmp/iter33/loose/i33e-1.0.0/")
    do ..Cmd("ld-g","load /tmp/iter33/loose/i33g-1.0.0/")
    // Installed graph: a 2.0.0 required by b and d; c requires b; g requires c.
    do ..Flat("l0","list-dependents i33a")
    do ..Flat("l0v1","list-dependents i33a 1.0.0")
    do ..Flat("l0v2","list-dependents i33a 2.0.0")
    do ..Tree("l0t","list-dependents -t i33a")
    do ..Flat("lb","list-dependents i33b")
    do ..Tree("lbt","list-dependents -t i33b")
    do ..Flat("ld","list-dependents i33d")
    do ..Tree("ldt","list-dependents -t i33d")
    do ..Flat("le","list-dependents i33e")
    // Repository form: manifests in r33 only, installed modules ignored.
    do ..Flat("lr","list-dependents -r r33 i33a")
    do ..Flat("lrv2","list-dependents -r r33 i33a 2.0.0")
    // Target removed while dependents stay: graphs must resolve i33a from r33 (1.0.0 only).
    do ..Cmd("un-a","uninstall i33a -force")
    do ..Flat("l2","list-dependents i33a")
    do ..Flat("l2v1","list-dependents i33a 1.0.0")
    do ..Cmd("un-d","uninstall i33d -force")
    do ..Flat("l3","list-dependents i33a")
    do ..Flat("l3v1","list-dependents i33a 1.0.0")
    write !,"NATIVE-END",!
}

}
