/// Test-only observer for iteration47. Not IPM code. Dumps device output and report files as base64
/// between markers; the host-side report_observer.py parses them.
Class IPMCont.ReportReader
{

Parameter OUT = "/tmp/iter47/out/";

ClassMethod Dump(pTag As %String, pText As %String, pRaw As %Boolean = 0)
{
    set b = $system.Encryption.Base64Encode($select(pRaw:pText,1:$zconvert(pText,"O","UTF8")),1)
    write !,"B64BEGIN|"_pTag,!
    for i=1:4000:$length(b) { write $extract(b,i,i+3999),! }
    write "B64END|"_pTag,!
}

ClassMethod ReadFile(pPath As %String, Output pExists As %Boolean) As %String
{
    set pExists = ##class(%File).Exists(pPath)
    quit:'pExists ""
    set s = ##class(%Stream.FileBinary).%New()
    do s.LinkToFile(pPath)
    quit s.Read(1000000)
}

ClassMethod Case(pTag As %String, pCommand As %String, pFile As %String = "")
{
    if pFile '= "" { do ##class(%File).Delete(pFile) }
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set text = ""
    for i=1:1:$get(lines) { set text = text_$get(lines(i))_$char(10) }
    write !,"OBS|"_pTag_".ok|"_$system.Status.IsOK(sc),!
    do ..Dump(pTag_".device",text)
    if pFile '= "" {
        set content = ..ReadFile(pFile,.exists)
        write !,"OBS|"_pTag_".file|"_exists,!
        do ..Dump(pTag_".filetext",content,1)
    }
}

ClassMethod All()
{
    set o = ..#OUT
    do ##class(%File).CreateDirectoryChain(o)
    set sc = ##class(%IPM.Main).Shell("load /tmp/iter47/r47 -dev")
    write !,"OBS|setup.ok|"_$system.Status.IsOK(sc),!
    do ..Case("json","test r47 -only -f json -verbose -output-file "_o_"r.json",o_"r.json")
    do ..Case("yaml","test r47 -only -f yaml -verbose -output-file "_o_"r.yaml",o_"r.yaml")
    do ..Case("toon","test r47 -only -f toon -verbose -output-file "_o_"r.toon",o_"r.toon")
    do ..Case("xml","test r47 -only -output-file "_o_"r.xml",o_"r.xml")
    do ..Case("yamlq","test r47 -only -f yaml")
    write !,"NATIVE-END",!
}

}
