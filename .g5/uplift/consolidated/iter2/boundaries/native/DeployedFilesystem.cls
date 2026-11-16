<?xml version="1.0" encoding="UTF-8"?>
<Export generator="IRIS" version="26" zv="IRIS for UNIX (Ubuntu Server LTS for ARM64 Containers) 2026.1 (Build 234U)" ts="2026-11-14 17:39:37">
<Class name="%IPM.Repo.Filesystem.Definition">
<Super>%IPM.Repo.Definition</Super>
<TimeChanged>67888,9993.034171086</TimeChanged>
<TimeCreated>67888,9993.034171086</TimeCreated>

<Parameter name="DISPLAYNAME">
<Type>STRING</Type>
<Default>Filesystem</Default>
</Parameter>

<Parameter name="MONIKER">
<Type>STRING</Type>
<Default>filesystem</Default>
</Parameter>

<Parameter name="MONIKERALIAS">
<Type>STRING</Type>
<Default>f,fs</Default>
</Parameter>

<Index name="RootIndex">
<Properties>Root</Properties>
<Unique>1</Unique>
</Index>

<Property name="Details">
<Type>%String</Type>
<SqlComputeCode>set {*} = {Root}_$case({Depth},0:"*",:$replace($justify(" ",{Depth}*2),"  ",$select($$$isWINDOWS:"*\",1:"*/"))_"module.xml")</SqlComputeCode>
<SqlComputed>1</SqlComputed>
<SqlComputeOnChange>%%INSERT,%%UPDATE</SqlComputeOnChange>
<Parameter name="MAXLEN"/>
</Property>

<Property name="Root">
<Description>
Root folder on the server filesystem</Description>
<Type>%String</Type>
<Required>1</Required>
<Parameter name="MAXLEN" value="260"/>
</Property>

<Property name="Depth">
<Description>
How many levels of depth to search for module.xml files; 0 indicates unlimited.</Description>
<Type>%Integer</Type>
<InitialExpression>0</InitialExpression>
<Required>1</Required>
</Property>

<Parameter name="RootPromptString">
<Description>
Prompt to use for Root in interactive configuration of this repository type</Description>
<Expression>$$$Text("Root File Path:","ZPM")</Expression>
</Parameter>

<XData name="Commands">
<Data><![CDATA[
<commands>
<command name="repo">
<group name="Filesystem repository">

<modifier name="filesystem" aliases="f,fs" description="Create/update a filesystem repository" />
<modifier name="depth" aliases="d" value="true" description="For filesystem repositories, specifies the depth (number of folders underneath the root) to search for files named module.xml" />
<modifier name="path" aliases="p" value="true" description="For filesystem repositories, specifies the path to the root directory" />

<example description="Create a repository pointing to the filesystem with name LocalFiles, accepting snapshots, looking for files named module.xml up to 2 directories deep in C:\MyWorkspace\RootModuleDir\">
repo -name LocalFiles -snapshots 1 -fs -depth 2 -path C:\MyWorkspace\RootModuleDir\
</example>

</group>
</command>
</commands>
]]></Data>
</XData>

<Method name="RootSet">
<FormalSpec>%val</FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    if (%val '= "") {
        set %val = ##class(%File).NormalizeDirectory(%val)
    }
    set i%Root = %val
    quit $$$OK
]]></Implementation>
</Method>

<Method name="GetPackageService">
<Description>
Returns a client to interact with this server.</Description>
<ReturnType>%IPM.Repo.IPackageService</ReturnType>
<Implementation><![CDATA[    return ##class(%IPM.Repo.Filesystem.PackageService).%New(..Root)
]]></Implementation>
</Method>

<Method name="OnConfigure">
<Description>
Handles modifiers/data attributes provided from the package manager shell.</Description>
<ClassMethod>1</ClassMethod>
<FormalSpec><![CDATA[pInstance:%IPM.Repo.Definition,pInteractive:%Boolean=1,&pModifiers,&pData]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tSC = $$$OK
    try {
        set tPath = $get(pModifiers("path"),pInstance.Root)
        if (pInteractive && (tPath = "")) {
            set tPrompt = ..#RootPromptString
            set tResponse = ##class(%Library.Prompt).GetString(tPrompt,.tPath)
            if (tResponse '= $$$SuccessResponse) {
                $$$ThrowStatus($$$ERROR($$$GeneralError,"Operation cancelled."))
            }

            if ##class(%IPM.Repo.Filesystem.Definition).RootIndexExists(tPath) {
                $$$ThrowStatus($$$ERROR($$$GeneralError,$$$FormatText("Root '%1' is already configured for another repository.",tPath)))
            }
        }
        set pInstance.Root = tPath

        if $data(pModifiers("depth"),tDepth) {
            set pInstance.Depth = tDepth
        }

        // This also saves it.
        $$$ThrowOnError(pInstance.BuildCache(1,1,1))
    } catch e {
        set tSC = e.AsStatus()
    }
    quit tSC
]]></Implementation>
</Method>

<Method name="GetPublishService">
<Description>
Returns a client to publish modules/applications to this server.</Description>
<ReturnType>%IPM.Repo.IPublishService</ReturnType>
<Implementation><![CDATA[
    // TODO: Implement? (Does it make sense to allow publishing to the local filesystem? What does that even mean?)
    quit $$$NULLOREF
]]></Implementation>
</Method>

<Method name="GetSortOrder">
<Private>1</Private>
<ReturnType>%Integer</ReturnType>
<Implementation><![CDATA[    quit 2
]]></Implementation>
</Method>

<Trigger name="RootChanged">
<Description>
Clears the cache if the root is changed.</Description>
<Code><![CDATA[    new tOldRoot
    if ({Root*C}) {
        set tOldRoot = {Root*O}
        &sql(delete from %IPM_Repo_Filesystem.Cache where Root = :tOldRoot)
        if (SQLCODE < 0) {
            set %ok = 0
        }
    }]]></Code>
<Event>UPDATE</Event>
<Foreach>row/object</Foreach>
</Trigger>

<Method name="BuildCache">
<FormalSpec>pPurge:%Boolean=1,pVerbose:%Integer=0,pAutoDetectDepth:%Boolean=0</FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tSC = $$$OK
    set tInitTLevel = $tlevel
    try {
        set tLogManager = ##class(%IPM.General.LogManager).%Get(.tSC)
        $$$ThrowOnError(tSC)

        // Instantiate at this level for the performance benefit
        set tXSLTProvider = ##class(%IPM.Repo.XSLTProvider).%Get(.tSC)
        $$$ThrowOnError(tSC)

        tstart
        if (pPurge) && (..%Id() '= "") {
            set tLockManager = ##class(%IPM.Utils.LockManager).%New()
            $$$ThrowOnError(tLockManager.LockClassId($classname(),..%Id()))
            #dim tResult As %SQL.StatementResult
            set tResult = ##class(%SQL.Statement).%ExecDirect(,"delete from %IPM_Repo_Filesystem.Cache where Root = ?",..Root)
            if (tResult.%SQLCODE < 0) {
                set tSC = $$$ERROR($$$SQLCode,tResult.%SQLCODE,tResult.%Message)
                quit
            }
        }

        // Force save so that root foreign key for items will exist.
        set tSC = ..%Save()
        if $$$ISERR(tSC) {
            quit
        }

        // Scan root directory recursively, up to ..Depth levels down, for module.xml files.
        set tSC = ..ScanDirectory(..Root,.tFilenameList,,..Depth,$select(pVerbose>1:1,1:0),.tMaxDepth)
        if $$$ISERR(tSC) {
            quit
        }

        if (pAutoDetectDepth) && (tMaxDepth > 0) {
            set ..Depth = tMaxDepth
            set tSC = ..%Save()
            if $$$ISERR(tSC) {
                quit
            }
        }

        // Ensure that we're looking at valid MODULE documents (as opposed to classes named Module, which the package manager has!)
        set tAggSC = $$$OK
        set tKey = ""
        for {
            set tKey = $order(tFilenameList(tKey),1,tFile)
            quit:(tKey="")

            set tSubDirectory = tFilenameList(tKey,"sub")
            set tAggSC = $$$ADDSC(tAggSC,..AddCacheItem(tFile,tSubDirectory,.tName,.tVersionString))
            write:pVerbose !,tName," ",tVersionString," @ ",##class(%File).NormalizeDirectory(..Root_tSubDirectory)
        }
        $$$ThrowOnError(tAggSC)
        tcommit
    } catch e {
        set tSC = e.AsStatus()
    }
    while ($tlevel > tInitTLevel) { trollback 1 }
    quit tSC
]]></Implementation>
</Method>

<Method name="AddCacheItem">
<FormalSpec>pModuleFileName:%String,pSubDirectory:%String,*pName:%String,*pVersionString:%String</FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tSC = $$$OK
    set pName = ""
    set pVersionString = ""
    try {
        // Get list of what's in module.xml
        set tSC = $system.OBJ.Load(pModuleFileName,"-d",,.tLoadedList,1)
        if $$$ISERR(tSC) {
            // Wasn't a valid file. We'll just continue.
            set tSC = $$$OK
            quit
        }

        if ($length(tLoadedList,",") > 1) {
            // Contained multiple documents - tricky! We'll just continue.
            quit
        }

        set tExt = $zconvert($piece($get(tLoadedList),".",*),"U")
        if (tExt '= "ZPM") {
            quit
        }

        kill tStream,tName,tVersionString
        set tParseSC = ..GetModuleStreamFromFile(pModuleFileName,.tStream,.pName,.pVersionString)
        if $$$ISERR(tParseSC) {
            // Log as a warning, but keep going.
            do ##class(%IPM.General.LogManager).Warning("Failed to parse module manifest in "_pModuleFileName_": "_$system.Status.GetErrorText(tParseSC),1)
            quit
        }

        // Create cache item.
        if ##class(%IPM.Repo.Filesystem.Cache).CacheItemIndexExists(..Root,pSubDirectory) {
            set tCacheItem = ##class(%IPM.Repo.Filesystem.Cache).CacheItemIndexOpen(..Root,pSubDirectory,,.tSC)
            $$$ThrowOnError(tSC)
        } else {
            set tCacheItem = ##class(%IPM.Repo.Filesystem.Cache).%New()
            set tCacheItem.Root = ..Root
            set tCacheItem.SubDirectory = pSubDirectory
        }
        set tCacheItem.Name = pName
        set tCacheItem.VersionString = pVersionString
        do tCacheItem.Manifest.CopyFrom(tStream)
        set tCacheItem.LastModified = $zdatetime($ztimestamp,3)
        set tSaveSC = tCacheItem.%Save()
        if $$$ISERR(tSaveSC) {
            set tSC = tCacheItem.HandleSaveError(tSaveSC)
        }
    } catch e {
        set tSC = e.AsStatus()
    }
    quit tSC
]]></Implementation>
</Method>

<Method name="GetModuleStreamFromFile">
<FormalSpec>pFilename:%String,*pStream:%Stream.GlobalCharacter,*pName:%String,*pVersion:%String</FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tSC = $$$OK
    set pName = ""
    set pVersion = ""
    try {
        // The source stream should contain a Studio document export of a module manifest.
        // Binary so the XSLT parser decodes per the encoding declaration, not the process's default file translation table
        set tSourceStream = ##class(%Stream.FileBinary).%New()
        set tSC = tSourceStream.LinkToFile(pFilename)
        $$$ThrowOnError(tSC)

        set pStream = ##class(%Stream.GlobalCharacter).%New()
        set tMetaStream = ##class(%Stream.GlobalCharacter).%New()

        // Extract <Module> section of document
        set tCompiledTransform = ##class(%IPM.Repo.XSLTProvider).GetCompiledTransformForXData($classname(),"ModuleDocumentTransform")
        set tSC = ##class(%XML.XSLT.Transformer).TransformStreamWithCompiledXSL(tSourceStream,tCompiledTransform,.pStream)
        $$$ThrowOnError(tSC)

        // Extract Name and Version
        do tSourceStream.Rewind()
        set tCompiledTransform = ##class(%IPM.Repo.XSLTProvider).GetCompiledTransformForXData($classname(),"MetadataExtractionTransform")
        set tSC = ##class(%XML.XSLT.Transformer).TransformStreamWithCompiledXSL(tSourceStream,tCompiledTransform,.tMetaStream)
        $$$ThrowOnError(tSC)

        set tMetaStream.LineTerminator = $select($$$isWINDOWS:$char(13,10),$$$isUNIX:$char(10))
        set pName = $zconvert(tMetaStream.ReadLine(), "L")
        set pVersion = tMetaStream.ReadLine()
    } catch e {
        set tOuterSC = $$$ERROR($$$GeneralError,$$$FormatText("Error loading module from file '%1'",pFilename))
        set tSC = $system.Status.EmbedStatus(tOuterSC,e.AsStatus())
    }
    quit tSC
]]></Implementation>
</Method>

<XData name="ModuleDocumentTransform">
<Description><![CDATA[
Extracts entire <Module> section from a Studio document export]]></Description>
<Data><![CDATA[
<xsl:stylesheet version="1.0"
 xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
 <xsl:output indent="yes"/>
 <xsl:template match="/Export/Document/Module">
 <Module>
 <xsl:copy-of select="node()"/>
 </Module>
 </xsl:template>
</xsl:stylesheet>
]]></Data>
</XData>

<XData name="MetadataExtractionTransform">
<Description>
Extracts Name and Version from a Studio document export and prints them one per line.</Description>
<Data><![CDATA[
<xsl:stylesheet version="1.0"
 xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
 <xsl:output method="text"/>
 <xsl:strip-space elements="*"/>
 <xsl:template match="/Export/Document/Module">
  <xsl:value-of select="Name" />
  <!-- this prints a newline. -->
  <xsl:text>&#10;</xsl:text>
  <xsl:value-of select="Version" />
 </xsl:template>
</xsl:stylesheet>
]]></Data>
</XData>

<Method name="ScanDirectory">
<Internal>1</Internal>
<ClassMethod>1</ClassMethod>
<FormalSpec><![CDATA[pRoot:%String,&pFilenameList,pSub:%String="",pDepth:%Integer="",pVerbose:%Boolean=0,*pMaxDepth:%Integer=0]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tSC = $$$OK
    try {
        set pRoot = ##class(%File).NormalizeDirectory(pRoot)
        set tDirArray($increment(tDirArray)) = ""

        set i = 0
        do {
            set i = i + 1
            if (pDepth > 0) && ($length(tDirArray(i),"/") > pDepth) {
                continue
            }

            set tStmt = ##class(%SQL.Statement).%New()
            set tSC = tStmt.%PrepareClassQuery("%Library.File","FileSet")
            if $$$ISERR(tSC) {
                quit
            }

            set tFullDir = ##class(%File).NormalizeDirectory(pRoot_tDirArray(i))
            set tRes = tStmt.%Execute(tFullDir,"module.xml",,1)
            while tRes.%Next() {
                if (tRes.%Get("Type") = "D") {
                    write:pVerbose !,"Scanning directory: ",tRes.%Get("ItemName")
                    set tDirArray($increment(tDirArray)) = tDirArray(i)_$case(tDirArray(i),"":"",:"/")_tRes.%Get("ItemName")
                } else {
                    write:pVerbose !,"Found file: ",tRes.%Get("Name")
                    set pFilenameList($increment(pFilenameList)) = tRes.%Get("Name")
                    set pFilenameList(pFilenameList,"sub") = tDirArray(i)
                    if (pDepth > pMaxDepth) {
                        set pMaxDepth = $length(tDirArray(i),"/")
                    }
                }
            }
        } while (i < tDirArray)
    } catch e {
        set tSC = e.AsStatus()
    }
    quit tSC
]]></Implementation>
</Method>

<XData name="LockFileMapping">
<Data><![CDATA[
<Mapping xmlns="http://www.intersystems.com/jsonmapping">
  <Property Name="LockFileType" FieldName="type" />
  <Property Name="OverriddenSortOrder" FieldName="overriddenSortOrder" />
  <Property Name="ReadOnly" FieldName="readOnly" />
  <Property Name="Root" FieldName="root" />
  <Property Name="Depth" FieldName="depth" />
</Mapping>
]]></Data>
</XData>

<Storage name="Default">
<Type>%Storage.Persistent</Type>
<DefaultData>FilesystemRepoDefinitionDefaultData</DefaultData>
<Data name="FilesystemRepoDefinitionDefaultData">
<Subscript>"FilesystemRepoDefinition"</Subscript>
<Value name="1">
<Value>Root</Value>
</Value>
<Value name="2">
<Value>Depth</Value>
</Value>
</Data>
</Storage>
</Class>
</Export>
