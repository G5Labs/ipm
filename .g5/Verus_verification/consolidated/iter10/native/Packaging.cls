Include %IPM.Common
Class Iter10.Packaging
{
ClassMethod Obs(key, value) { write !,"OBS|",key,"|",value,! }
ClassMethod Cmd(key, command) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(command)
 do ..Obs(key,$$$ISOK(sc))
 if $$$ISERR(sc) { write !,"RAW|",key,"|",$system.Status.GetErrorText(sc),! }
 return $$$ISOK(sc)
}
ClassMethod Read(path) As %String
{
 set stream=##class(%Stream.FileCharacter).%New()
 $$$ThrowOnError(stream.LinkToFile(path))
 return stream.Read(32000)
}
ClassMethod BinaryEqual(a,b) As %Boolean
{
 set left=##class(%Stream.FileBinary).%New(),right=##class(%Stream.FileBinary).%New()
 $$$ThrowOnError(left.LinkToFile(a))
 $$$ThrowOnError(right.LinkToFile(b))
 if left.Size'=right.Size {return 0}
 while 'left.AtEnd {if left.Read(32000)'=right.Read(32000) {return 0}}
 return right.AtEnd
}
ClassMethod Write(path, text)
{
 set stream=##class(%Stream.FileCharacter).%New()
 $$$ThrowOnError(stream.LinkToFile(path))
 $$$ThrowOnError(stream.Write(text))
 $$$ThrowOnError(stream.%Save())
}
ClassMethod Source(label, manifest As %Integer, phase As %String) As %String
{
 set root="/tmp/iter10/packaging/work/"_label_"/"
 set original="/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/test-output-format/"
 $$$ThrowOnError(##class(%IPM.Utils.File).CopyDir(original,root))
 set xml=..Read(root_"module.xml")
 if manifest=1 {set xml=$replace(xml,"</Module>","<IncludeTests>1</IncludeTests></Module>")}
 if manifest=2 {set xml=$replace(xml,"</Module>","<IncludeTests>0</IncludeTests></Module>")}
 if phase="verify" {set xml=$replace(xml,"Phase=""test""","Phase=""verify""")}
 do ..Write(root_"module.xml",xml)
 return root
}
ClassMethod Filters()
{
 set module=##class(%IPM.Storage.Module).NameOpen("test-output-format",,.sc)
 $$$ThrowOnError(sc)
 set resource=module.Resources.GetAt(1)
 $$$ThrowOnError(resource.ResolveChildren(.direct))
 do ..Obs("ut-direct-path",$data(direct("/tests/"))>0)
 do ..Obs("ut-direct-scope",$data(direct("Scope"))>0)
 set resource.Scope="verify"
 $$$ThrowOnError(resource.%Save())
 kill resource,module
 set module=##class(%IPM.Storage.Module).NameOpen("test-output-format",,.sc)
 $$$ThrowOnError(sc)
 $$$ThrowOnError(module.GetResolvedReferences(.test,0,$listbuild("Test")))
 do ..Obs("ut-filter-resource",$data(test(module.Name,"/tests/"))>0)
 $$$ThrowOnError(module.GetResolvedReferences(.verify,0,$listbuild("Verify")))
 do ..Obs("ut-filter-returned",$data(verify(module.Name,"/tests/"))>0)
 $$$ThrowOnError(module.GetResolvedReferences(.both,0,$listbuild("Test","Verify")))
 do ..Obs("ut-filter-both",$data(both(module.Name,"/tests/"))>0)
 do ..Obs("ut-meta-leak",$data(both(module.Name,"Scope"))>0)
 do ..Obs("ut-path-processor",$data(both(module.Name,"/tests/","Processor"))>0)
 set resource=module.Resources.GetAt(1)
 set sc=resource.Processor.OnExportItem("/tmp/iter10/packaging/unused","/tests/",.itemParams,.params,.handled)
 do ..Obs("ut-path-export-ok",$$$ISOK(sc)),..Obs("ut-path-handled",handled)
 set resource.Scope=""
 $$$ThrowOnError(resource.%Save())
}
ClassMethod Archive(label, path, source)
{
 set tarfile=##class(%SYS.Python).Import("tarfile"),hashlib=##class(%SYS.Python).Import("hashlib"),pathlib=##class(%SYS.Python).Import("pathlib")
 set archive=tarfile."open"(path,"r:gz"),names=archive."getnames"()
 write !,"ARCHIVE|",label,"|",names."__str__"(),!
 set selected="tests/Test/Output/Format/AfterAllReturn.cls"
 set dest="/tmp/iter10/packaging/extracted/"_label_"/"
 set extract=##class(%IPM.General.Archive).Extract(path,dest)
 do ..Obs(label_"-extract",$$$ISOK(extract))
 set present=##class(%File).Exists(dest_selected)
 do ..Obs(label_"-test",present)
 do ..Obs(label_"-manifest",##class(%File).Exists(dest_"module.xml"))
 set equal=0
 if present {
  set equal=..BinaryEqual(dest_selected,source_selected)
 }
 do ..Obs(label_"-bytes",equal)
 do archive."close"()
}
ClassMethod One(label, manifest, phase, flags="", filters As %Boolean=0)
{
 set root=..Source(label,manifest,phase)
 do ..Cmd(label_"-load","load "_root)
 if filters {do ..Filters()}
 set stored=##class(%IPM.Storage.Module).NameOpen("test-output-format")
 do ..Obs(label_"-stored",+stored.IncludeTests)
 kill stored
 set output="/tmp/iter10/packaging/archives/"_label
 do ..Cmd(label_"-ok","package test-output-format -only -path "_output_flags)
 do ..Archive(label,output_".tgz",root)
}
ClassMethod Run()
{
 do ..One("pkg-default",0,"test","",1)
 do ..One("pkg-flag",0,"test"," -include-tests")
 do ..One("pkg-manifest",1,"test")
 do ..One("pkg-zero",1,"test"," -DIncludeTests=0")
 do ..One("pkg-sticky",0,"test")
 do ..One("pkg-verify-flag",2,"verify"," -include-tests")
 do ..One("pkg-verify-default",2,"verify")
}
}
