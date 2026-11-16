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
ClassMethod ProductReader(label,m,kind)
{
 set class=$select(kind="implicit":"Iter25.Value",1:"Iter25.Explicit"),name=$select(kind="implicit":"iter25-source",1:"iter25-explicit"),expected="/tmp/iter25/repo/"_name_"/src/"_$select(kind="implicit":"",1:"code/")_$translate(class,".","/")_".cls"
 set ref=##class(%IPM.Storage.ResourceReference).NonNullResourceNameOpen(class_".CLS")
 do ..Obs(label_"-resource-ref",$isobject(ref))
 if '$isobject(ref) {return}
 do ..Obs(label_"-resource-module",ref.Module.Name=name)
 set processor=ref.Processor
 do ..Obs(label_"-processor",$isobject(processor))
 if '$isobject(processor) {return}
 set sc=processor.GetSourceControlInfo(.info),relative=processor.OnItemRelativePath(ref.Name)
 do ..Obs(label_"-info-status",$$$ISOK(sc)),..Obs(label_"-info-aware",info.SourceControlAware)
 do ..Obs(label_"-info-type",info.ResourceType="cls"),..Obs(label_"-info-prefix",info.Prefix=class)
 write !,"PRODUCT-INFO|",label,"|",..Hex(info.RelativePath),"|",info.Prefix,"|",info.ResourceType,!
 set full=##class(%File).NormalizeFilename(m.Root_relative),exists=##class(%File).Exists(full)
 do ..Obs(label_"-product-path",full=expected),..Obs(label_"-product-file",exists)
 write !,"PRODUCT-PATH|",label,"|",relative,"|",full,!
 if exists {
  set stream=##class(%Stream.FileCharacter).%New(),stream.Filename=full,text=stream.Read()
  do ..Obs(label_"-product-source-class",$find(text,"Class "_class)>0),..Obs(label_"-product-source-payload",$find(text,"quit 1")>0)
 }
}
ClassMethod State(label,kind="implicit")
{
 set class=$select(kind="implicit":"Iter25.Value",1:"Iter25.Explicit"),name=$select(kind="implicit":"iter25-source",1:"iter25-explicit"),root="/tmp/iter25/repo/"_name_"/"
 set m=##class(%IPM.Storage.Module).NameOpen(name)
 do ..Obs(label_"-present",$isobject(m))
 if $isobject(m) {
  do ..Obs(label_"-mode",+m.DeveloperMode),..Obs(label_"-version",m.VersionString="1.0.0"),..Obs(label_"-supplier",m.Repository="alpha")
  do ..Obs(label_"-root",##class(%File).NormalizeDirectory(m.Root)=##class(%File).NormalizeDirectory(root))
 }
 set code=$system.CLS.IsMthd(class,"Who")>0
 do ..Obs(label_"-code",code)
 if code {do ..Obs(label_"-payload",$classmethod(class,"Who"))}
 set map=$get(^Sources("cls",class)),module=$get(^Sources("MODULE",name))
 do ..Obs(label_"-class-map",$data(^Sources("cls",class))#2),..Obs(label_"-module-map",$data(^Sources("MODULE",name))#2)
 do ..Obs(label_"-module-no-folders",+$get(^Sources("MODULE","*","NoFolders")))
 do ..Obs(label_"-class-map-prefix",$ascii(map)=8),..Obs(label_"-module-map-prefix",$ascii(module)=8)
 write !,"MAP|",label,"|",..Hex(map),"|",..Hex(module),!
 do ..ProductReader(label,m,kind)
 do ..Cmd("show-"_label,"show "_name)
}
ClassMethod Run()
{
 do ..Cmd("repo-clear","repo -delete-all")
 do ..Cmd("repo-alpha","repo -fs -name alpha -depth 1 -path /tmp/iter25/repo/")
 do ..Cmd("install-normal","install alpha/iter25-source 1.0.0"),..State("normal")
 do ..Cmd("reinstall-dev","reinstall iter25-source -dev"),..State("dev")
 do ..Cmd("reinstall-normal-after-dev","reinstall iter25-source"),..State("refused")
 do ..Cmd("install-explicit-normal","install alpha/iter25-explicit 1.0.0"),..State("explicit-normal","explicit")
 do ..Cmd("reinstall-explicit-dev","reinstall iter25-explicit -dev"),..State("explicit-dev","explicit")
 do ..Cmd("reinstall-explicit-normal-after-dev","reinstall iter25-explicit"),..State("explicit-refused","explicit")
 do ..Obs("workflow-complete",1),..Obs("final-tlevel",$tlevel)
}
}
