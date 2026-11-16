Class Iter6.Probe Extends %RegisteredObject
{
Property TempDir As %String;
Method Save(t)
{
 do t.ProcessResources()
 $$$ThrowOnError(t.SaveFile(..TempDir))
}
Method Summarize(elementName, a, b="") As %String
{
 $$$ThrowOnError(##class(%XML.TextReader).ParseFile(..TempDir_"module.xml",.reader))
 while reader.Read() {
  continue:((reader.NodeType'="element") || (reader.LocalName'=elementName))
  kill attrs
  for i=1:1:reader.AttributeCount {
   do reader.MoveToAttributeIndex(i)
   set attrs(reader.LocalName)=reader.Value
  }
  set line=$get(attrs("Name"))_" "_a_"="_$get(attrs(a))
  if b'="" {
   set line=line_" "_b_"="_$get(attrs(b))
  }
  set lines(line)=""
 }
 set k="",result=""
 for {
  set k=$order(lines(k))
  quit:k=""
  set result=result_$select(result="":"",1:", ")_k
 }
 quit result
}
ClassMethod Obs(k, v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 quit $$$ISOK(sc)
}
ClassMethod Run()
{
 set p=..%New(), p.TempDir="/tmp/iter6/generated/"
 do ##class(%IPM.Utils.File).CreateDirectoryChain(p.TempDir)
 set root="/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/"
 set t=##class(%IPM.Storage.ModuleTemplate).NewTemplate(root_"generate/unit-tests/","iter6-generated","1.0.0")
 do t.ReadResources("","tests"), p.Save(t)
 do ..Obs("template-app",p.Summarize("Resource","Directory")="GenFixture.App.PKG Directory=src/cls")
 do ..Obs("template-test",p.Summarize("UnitTest","Package","Phase")="/tests/unit_tests/ Package=GenFixture.Test.Unit Phase=test")
 set t=##class(%IPM.Storage.ModuleTemplate).NewTemplate(root_"generate/unit-tests/","iter6-missing","1.0.0"), failed=0
 try { do t.ReadResources("missing") } catch e { set failed=$system.Status.GetErrorText(e.AsStatus())["doesn't exist" }
 do ..Obs("template-missing",failed)
 do ##class(%IPM.Utils.File).CreateDirectoryChain("/tmp/iter6/copy/source/sub/")
 set f=##class(%Stream.FileCharacter).%New() do f.LinkToFile("/tmp/iter6/copy/source/sub/file.txt"), f.Write("payload"), f.%Save()
 do ..Obs("copy-child",$$$ISOK(##class(%IPM.Utils.File).CopyDir("/tmp/iter6/copy/source/","/tmp/iter6/copy/source/sub/")))
 do ..Obs("copy-parent",$$$ISOK(##class(%IPM.Utils.File).CopyDir("/tmp/iter6/copy/source/sub/","/tmp/iter6/copy/source/")))
 do ..Obs("copy-intact",##class(%File).Exists("/tmp/iter6/copy/source/sub/file.txt"))
 do ..Obs("copy-sibling",$$$ISOK(##class(%IPM.Utils.File).CopyDir("/tmp/iter6/copy/source/","/tmp/iter6/copy/target/")))
 do ..Obs("copy-nested",##class(%File).Exists("/tmp/iter6/copy/target/sub/file.txt"))
 do ..Obs("global-load",..Cmd("load "_root_"globals/"))
 do ..Obs("global-package",..Cmd("globals-test package -DPath=/tmp/iter6/globalpkg"))
 do ..Obs("global-extract",$$$ISOK(##class(%IPM.General.Archive).Extract("/tmp/iter6/globalpkg.tgz","/tmp/iter6/global-out/")))
 do ..Obs("global-path",##class(%File).Exists("/tmp/iter6/global-out/src/gbl/My.Settings.xml"))
 do ..Obs("global-uninstall",..Cmd("uninstall globals-test"))
 kill ^My.Settings
 do ..Obs("global-cleared",$get(^My.Settings("Parameter"),0))
 do ..Obs("global-reload",..Cmd("load /tmp/iter6/globalpkg.tgz"))
 do ..Obs("global-value",$get(^My.Settings("Parameter"),0))
 do ..Obs("alias-load",..Cmd("load "_root_"ext-name-mod-action/"))
 do ..Obs("alias-internal",..Cmd("default-name-mod-action greeting -only"))
 do ..Obs("alias-external",..Cmd("ext-name-mod-action greeting -only"))
 do ..Obs("alias-package",..Cmd("ext-name-mod-action package -only -DPath=/tmp/iter6/aliaspkg"))
 do ..Obs("phase-load",..Cmd("load "_root_"custom-phase-without-lifecycle/"))
 do ..Obs("phase-explicit",..Cmd("custom-phase-without-lifecycle greeting -only"))
 do ..Obs("phase-uninstall",..Cmd("uninstall custom-phase-without-lifecycle"))
 do ..Obs("zero-load",..Cmd("load "_root_"zero-byte-file/"))
 do ..Obs("zero-package",..Cmd("zero-byte-file package -only -DPath=/tmp/iter6/zeropkg"))
 do ..Obs("zero-extract",$$$ISOK(##class(%IPM.General.Archive).Extract("/tmp/iter6/zeropkg.tgz","/tmp/iter6/zero-out/")))
 do ..Obs("zero-exists",##class(%File).Exists("/tmp/iter6/zero-out/src/empty.txt"))
 do ..Obs("zero-size",##class(%File).GetFileSize("/tmp/iter6/zero-out/src/empty.txt"))
 do ..Obs("zero-later",##class(%File).Exists("/tmp/iter6/zero-out/src/nonempty.txt"))
}
}
