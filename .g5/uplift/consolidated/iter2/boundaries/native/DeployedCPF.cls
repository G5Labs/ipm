<?xml version="1.0" encoding="UTF-8"?>
<Export generator="IRIS" version="26" zv="IRIS for UNIX (Ubuntu Server LTS for ARM64 Containers) 2026.1 (Build 234U)" ts="2026-11-14 17:39:37">
<Class name="%IPM.ResourceProcessor.CPF">
<IncludeCode>%IPM.Formatting</IncludeCode>
<Super>%IPM.ResourceProcessor.Abstract,%IPM.ResourceProcessor.CustomPhaseMixin</Super>
<TimeChanged>67888,9993.093738586</TimeChanged>
<TimeCreated>67888,9993.093738586</TimeCreated>

<Parameter name="ATTRIBUTES">
<Description>
Comma-separated list of resource attribute names that this processor uses</Description>
<Type>STRING</Type>
<Default>Name,Directory,Phase,CustomPhase,When</Default>
</Parameter>

<Parameter name="DESCRIPTION">
<Description>
Description of resource processor class (shown in UI)</Description>
<Type>STRING</Type>
<Default>Merges the specified CPF file in the specified lifecycle phase ("Initialize" by default).</Default>
</Parameter>

<Property name="Directory">
<Description>
Directory containing the CPF file to merge</Description>
<Type>%IPM.DataType.ResourceDirectory</Type>
<InitialExpression>"cpf"</InitialExpression>
</Property>

<Property name="Name">
<Description>
FileN ame of the CPF merge file</Description>
<Type>%IPM.DataType.ResourceName</Type>
<Required>1</Required>
</Property>

<Property name="Phase">
<Description>
The phase before/after which the CPF file should be merged</Description>
<Type>%IPM.DataType.PhaseName</Type>
<InitialExpression>"Initialize"</InitialExpression>
</Property>

<Property name="When">
<Description>
When to merge the CPF file: Before or After the specified phase. This only applies to the standard phases.</Description>
<Type>%IPM.DataType.PhaseWhen</Type>
<InitialExpression>"Before"</InitialExpression>
</Property>

<Method name="OnBeforePhase">
<FormalSpec><![CDATA[pPhase:%String,&pParams]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    if (..When = "Before") && (..Phase = pPhase) && (..CustomPhase = "") {
        quit ..DoMerge(.pParams)
    }
    quit $$$OK
]]></Implementation>
</Method>

<Method name="OnAfterPhase">
<FormalSpec><![CDATA[pPhase:%String,&pParams]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    if (..When = "After") && (..Phase = pPhase) && (..CustomPhase = "") {
        quit ..DoMerge(.pParams)
    }
    quit $$$OK
]]></Implementation>
</Method>

<Method name="OnCustomPhase">
<FormalSpec><![CDATA[pCustomPhase:%String,&pParams]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    if (..CustomPhase = pCustomPhase) {
        quit ..DoMerge(.pParams)
    }
    quit $$$OK
]]></Implementation>
</Method>

<Method name="OnPhase">
<FormalSpec><![CDATA[pPhase:%String,&pParams,*pResourceHandled:%Boolean=0]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    // CPF resources don't need any cleanup during Clean phase
    // Mark as handled to prevent the generic resource cleanup code from trying to delete them
    // as if they were classes/routines, which causes error `#5006: File name is invalid`
    if (pPhase = "Clean") {
        set pResourceHandled = 1
    }
    quit $$$OK
]]></Implementation>
</Method>

<Method name="OnResolveChildren">
<Description><![CDATA[
Steers the per-item export to <SourcesRoot>/<Directory>/<Name> so the file lands
where DoMerge will look for it after install.]]></Description>
<FormalSpec><![CDATA[&pResourceArray,pCheckModuleOwnership:%Boolean]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set key = $order(pResourceArray(""))
    if (key = "") {
        quit $$$ERROR($$$GeneralError, "Empty resource array for CPF resource processor: " _ ..Name)
    }
    set sourcesRoot = ..ResourceReference.Module.SourcesRoot
    set pResourceArray(key, "RelativePath") = ##class(%File).Construct(sourcesRoot, ..Directory, ..Name)
    quit $$$OK
]]></Implementation>
</Method>

<Method name="OnExportItem">
<Description>
Copies the CPF file into the export directory. The default routine export path fails for
CPF resources with `ERROR #6080: Can not export 'CPF' type in %RO format`.</Description>
<FormalSpec><![CDATA[pFullExportPath:%String,pItemName:%String,&pItemParams,&pParams,*pItemHandled:%Boolean=0]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set pItemHandled = 0
    set verbose = $get(pParams("Verbose"))
    set source = ..GetSourceFile()

    set destDir = ##class(%File).GetDirectory(pFullExportPath)
    if ##class(%File).Exists(destDir) && '##class(%File).DirectoryExists(destDir) {
        quit $$$ERROR($$$GeneralError, "File '" _ destDir _ "' exists and is not a directory. Failed to export CPF item: " _ ..Name)
    }
    if '##class(%File).DirectoryExists(destDir) {
        $$$ThrowOnError(##class(%IPM.Utils.File).CreateDirectoryChain(destDir))
    }
    if verbose {
        write !, "Copying CPF item '" _ source _ "' to '" _ pFullExportPath _ "'"
    }
    set pItemHandled = ##class(%File).CopyFile(source, pFullExportPath, 1, .return)
    if 'pItemHandled {
        quit $$$ERROR($$$GeneralError, "Failed to copy " _ source _ " to " _ pFullExportPath _ ", OS returned code: " _ -return)
    }
    quit $$$OK
]]></Implementation>
</Method>

<Method name="GetSourceFile">
<Description>
Resolves the absolute path of the CPF source file. Throws if the file is missing.</Description>
<Private>1</Private>
<ReturnType>%String</ReturnType>
<Implementation><![CDATA[
    set root = ..ResourceReference.Module.Root
    set sourcesRoot = ..ResourceReference.Module.SourcesRoot
    set dir = ##class(%File).Construct(root, sourcesRoot, ..Directory)
    set filename = ##class(%File).NormalizeFilename(..Name, dir)
    if (filename = "") || ('##class(%File).Exists(filename)) {
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("CPF file '%1' not found in directory '%2'", ..Name, dir)))
    }
    return filename
]]></Implementation>
</Method>

<Method name="DoMerge">
<FormalSpec><![CDATA[&pParams]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tempFile = ""
    set sc = $$$OK
    try {
        set verbose = $get(pParams("Verbose"))
        set filename = ..GetSourceFile()

        // Read the CPF file and evaluate system expressions
        set inputStream = ##class(%Stream.FileCharacter).%New()
        $$$ThrowOnError(inputStream.LinkToFile(filename))

        // Create a temporary file for the processed CPF
        set tempFileManager = ##class(%IPM.Utils.TempFileManager).%Get(.status)
        $$$ThrowOnError(status)
        set tempFile = tempFileManager.GetTempFileName("cpf")
        if verbose {
            write !, "Evaluating system expressions using temporary file at " _ tempFile,!
        }
        set outputStream = ##class(%Stream.FileCharacter).%New()
        set outputStream.Filename = tempFile

        // Process each line, evaluating system expressions
        while 'inputStream.AtEnd {
            set line = inputStream.ReadLine()
            set processedLine = ##class(%IPM.Utils.Module).%EvaluateSystemExpression(line)
            set processedLine = ##class(%IPM.Utils.Module).%EvaluateMacro(processedLine)
            $$$ThrowOnError(outputStream.WriteLine(processedLine))
        }
        $$$ThrowOnError(outputStream.%Save())

        if verbose {
            write !, "Merging CPF file: ", filename, !
            // OutputToDevice() without a len argument will rewind to the beginning of the stream
            $$$ThrowOnError(outputStream.OutputToDevice())
        }

        do ..MergeCPF(tempFile, filename)
    } catch ex {
        set sc = ex.AsStatus()
    }

    return sc
]]></Implementation>
</Method>

<Method name="MergeCPF">
<Description>
Merges file into the running instance. sourceFile is the name reported on failure,
since file may be a processed temporary copy. Defaults to file.</Description>
<ClassMethod>1</ClassMethod>
<FormalSpec>file:%String,sourceFile:%String=""</FormalSpec>
<Implementation><![CDATA[
    // TODO The $zf(-100) callout is much slower than ##class(Config.CPF).Merge()
    //      Figure out why ##class(Config.CPF).Merge() doesn't work
    //      c.f. https://github.com/intersystems/ipm/pull/703#discussion_r1917290136

    set args($increment(args)) = "merge"
    set args($increment(args)) = ##class(%SYS.System).GetInstanceName()
    set args($increment(args)) = file

    // Somehow, if the STDOUT is not set, the merge will silently fail
    set flags = "/SHELL/LOGCMD/STDOUT=""zf100stdout""/STDERR=""zf100stderr"""
    // Keeps the child from opening the caller's terminal
    set flags = flags _ "/STDIN=""" _ ##class(%Library.Device).GetNullDevice() _ """"
    set status = $zf(-100, flags, "iris", .args)
    if status '= 0 {
        set msg = $$$FormatText("Error merging CPF file '%1'. $zf(-100) exited with %2", $select(sourceFile = "": file, 1: sourceFile), status)
        set details = ..ReadOutputFile("zf100stdout") _ ..ReadOutputFile("zf100stderr")
        set details = details _ $char(13, 10) _ "See messages.log for details."
        $$$ThrowStatus($$$ERROR($$$GeneralError, msg _ details))
    }
]]></Implementation>
</Method>

<Method name="ReadOutputFile">
<Description>
Returns the non-blank lines of a merge output file in the process's current directory, each preceded by a newline.</Description>
<ClassMethod>1</ClassMethod>
<FormalSpec>name:%String</FormalSpec>
<Private>1</Private>
<ReturnType>%String</ReturnType>
<Implementation><![CDATA[
    set path = ##class(%File).NormalizeFilename(name, ##class(%SYSTEM.Process).CurrentDirectory())
    if '##class(%File).Exists(path) {
        return ""
    }
    set stream = ##class(%Stream.FileCharacter).%New()
    if $$$ISERR(stream.LinkToFile(path)) {
        return ""
    }
    set text = ""
    while 'stream.AtEnd {
        set line = $zstrip(stream.ReadLine(), "<>W")
        if line '= "" {
            set text = text _ $char(13, 10) _ line
        }
    }
    return text
]]></Implementation>
</Method>
</Class>
</Export>
