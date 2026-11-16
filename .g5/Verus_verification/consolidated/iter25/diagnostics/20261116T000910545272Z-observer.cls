Include %IPM.Common
Class Iter25.DeveloperSources
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Hex(s)
{
 set h="",digits="0123456789abcdef"
 for i=1:1:$length(s) {
  set n=$ascii(s,i),h=h_$extract(digits,(n\16)+1)_$extract(digits,(n#16)+1)
 }
 return h
}
ClassMethod Cmd(label,cmd)
{
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(cmd)
 write !,"END|",label,!
 do ..Obs(label,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"ERROR|",label,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod ProductReader(label,m)
{
 set ref=##class(%IPM.Storage.ResourceReference).NonNullResourceNameOpen("Iter25.Value.CLS")
 do ..Obs(label_"-resource-ref",$isobject(ref))
 if '$isobject(ref) {return}
 do ..Obs(label_"-resource-module",ref.Module.Name="iter25-source")
 set processor=ref.Processor
 do ..Obs(label_"-processor",$isobject(processor))
 if '$isobject(processor) {return}
 set sc=processor.GetSourceControlInfo(.info),relative=processor.OnItemRelativePath(ref.Name)
 do ..Obs(label_"-info-status",$$$ISOK(sc)),..Obs(label_"-info-aware",info.SourceControlAware)
 do ..Obs(label_"-info-type",info.ResourceType="cls"),..Obs(label_"-info-prefix",info.Prefix="Iter25.Value")
 write !,"PRODUCT-INFO|",label,"|",..Hex(info.RelativePath),"|",info.Prefix,"|",info.ResourceType,!
 set full=##class(%File).NormalizeFilename(m.Root_relative),exists=##class(%File).Exists(full)
 do ..Obs(label_"-product-path",full="/tmp/iter25/repo/iter25-source/src/Iter25/Value.cls"),..Obs(label_"-product-file",exists)
 write !,"PRODUCT-PATH|",label,"|",relative,"|",full,!
 if exists {
  set stream=##class(%Stream.FileCharacter).%New(),stream.Filename=full,text=stream.Read()
  do ..Obs(label_"-product-source-class",$find(text,"Class Iter25.Value")>0),..Obs(label_"-product-source-payload",$find(text,"quit 1")>0)
 }
}
ClassMethod State(label)
{
 set m=##class(%IPM.Storage.Module).NameOpen("iter25-source")
 do ..Obs(label_"-present",$isobject(m))
 if $isobject(m) {
  do ..Obs(label_"-mode",+m.DeveloperMode),..Obs(label_"-version",m.VersionString="1.0.0"),..Obs(label_"-supplier",m.Repository="alpha")
  do ..Obs(label_"-root",##class(%File).NormalizeDirectory(m.Root)=##class(%File).NormalizeDirectory("/tmp/iter25/repo/iter25-source/"))
 }
 set code=$system.CLS.IsMthd("Iter25.Value","Who")>0
 do ..Obs(label_"-code",code)
 if code {do ..Obs(label_"-payload",##class(Iter25.Value).Who())}
 set map=$get(^Sources("cls","Iter25.Value")),module=$get(^Sources("MODULE","iter25-source"))
 do ..Obs(label_"-class-map",$data(^Sources("cls","Iter25.Value"))#2),..Obs(label_"-module-map",$data(^Sources("MODULE","iter25-source"))#2)
 do ..Obs(label_"-module-no-folders",+$get(^Sources("MODULE","*","NoFolders")))
 do ..Obs(label_"-class-map-prefix",$ascii(map)=8),..Obs(label_"-module-map-prefix",$ascii(module)=8)
 write !,"MAP|",label,"|",..Hex(map),"|",..Hex(module),!
 set readerOK=0,path=""
 try {
  set path=##class(%Studio.SourceControl.ISC).ExtName("Iter25.Value.CLS"),readerOK=1
 } catch ex {write !,"READER-ERROR|",label,"|",ex.DisplayString(),!}
 set lower="",external="",externalOK=0
 try {set lower=##class(%Studio.SourceControl.ISC).ExtName("Iter25.Value.cls")} catch ex {write !,"LOWER-ERROR|",label,"|",ex.DisplayString(),!}
 try {
  set sdk=##class(%Studio.SourceControl.ISC).%New("")
  set external=sdk.ExternalName("Iter25.Value.cls"),externalOK=1
 } catch ex {write !,"EXTERNAL-ERROR|",label,"|",ex.DisplayString(),!}
 do ..Obs(label_"-lower-reader-source",##class(%File).NormalizeFilename(lower)="/tmp/iter25/repo/iter25-source/src/Iter25/Value.cls")
 do ..Obs(label_"-external-reader-ok",externalOK),..Obs(label_"-external-reader-source",##class(%File).NormalizeFilename(external)="/tmp/iter25/repo/iter25-source/src/Iter25/Value.cls")
 write !,"LOWER-PATH|",label,"|",lower,!,"EXTERNAL-PATH|",label,"|",external,!
 do ..Obs(label_"-reader-ok",readerOK)
 do ..Obs(label_"-reader-source",##class(%File).NormalizeFilename(path)="/tmp/iter25/repo/iter25-source/src/Iter25/Value.cls")
 do ..Obs(label_"-reader-exists",##class(%File).Exists(path))
 write !,"SOURCE-PATH|",label,"|",path,!
 do ..ProductReader(label,m)
 do ..Cmd("show-"_label,"show iter25-source")
}
ClassMethod Run()
{
 do ..Cmd("repo-clear","repo -delete-all")
 do ..Cmd("repo-alpha","repo -fs -name alpha -depth 1 -path /tmp/iter25/repo/")
 do ..Cmd("install-normal","install alpha/iter25-source 1.0.0"),..State("normal")
 do ..Cmd("reinstall-dev","reinstall iter25-source -dev"),..State("dev")
 do ..Cmd("reinstall-normal-after-dev","reinstall iter25-source"),..State("refused")
 do ..Obs("workflow-complete",1),..Obs("final-tlevel",$tlevel)
}
}
