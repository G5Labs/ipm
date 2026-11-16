<?xml version="1.0" encoding="UTF-8"?>
<Export generator="IRIS" version="26" zv="IRIS for UNIX (Ubuntu Server LTS for ARM64 Containers) 2026.1 (Build 234U)" ts="2026-11-14 17:39:37">
<Class name="%IPM.General.HistoryTemp">
<Description>
Temporary version of history log that lives in IRISTEMP to avoid being affected by TROLLBACK
Persisted fully by being converted to %IPM.General.History</Description>
<Super>%IPM.General.AbstractHistory</Super>
<TimeChanged>67888,9992.896878419</TimeChanged>
<TimeCreated>67888,9992.896878419</TimeCreated>

<Property name="PersistedTwin">
<Type>%IPM.General.History</Type>
</Property>

<Method name="Init">
<Description>
Initalize both this temporary version and the persistent version</Description>
<ClassMethod>1</ClassMethod>
<FormalSpec>Action:%String,Package:%IPM.DataType.ModuleName</FormalSpec>
<Private>1</Private>
<ReturnType>%IPM.General.HistoryTemp</ReturnType>
<Implementation><![CDATA[
    set log = ##super(Action, Package)
    set twin = ##class(%IPM.General.History).%New()
    set twin.TimeStart = log.TimeStart
    set twin.Action = log.Action
    set twin.Package = log.Package
    set twin.CommandString = log.CommandString
    set twin.UserName = log.UserName
    set twin.Version = log.Version
    $$$ThrowOnError(twin.%Save())

    set log.PersistedTwin = twin
    $$$ThrowOnError(log.%Save())
    quit log
]]></Implementation>
</Method>

<Method name="PersistToTwin">
<Description>
Explicitly persist current state to the PersistedTwin
Can be called before classes are deleted during self-uninstall</Description>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set sc = $$$OK
    if $$$comClassDefined("%IPM.General.History") {
        if (..PersistedTwin '= "") {
            // Reload to get latest state
            $$$ThrowOnError(..%Reload())

            // copy all values to the Persisted Twin
            set ..PersistedTwin.TimeStart = ..TimeStart
            set ..PersistedTwin.TimeEnd = ..TimeEnd
            set ..PersistedTwin.Action = ..Action
            set ..PersistedTwin.Package = ..Package
            set ..PersistedTwin.CommandString = ..CommandString
            set ..PersistedTwin.UserName = ..UserName
            set ..PersistedTwin.Version = ..Version
            set ..PersistedTwin.SourceName = ..SourceName
            set ..PersistedTwin.SourceMoniker = ..SourceMoniker
            set ..PersistedTwin.SourceDetails = ..SourceDetails
            set ..PersistedTwin.Success = ..Success
            set ..PersistedTwin.Committed = ..Committed
            set ..PersistedTwin.Phases = ..Phases
            // Mark as finalized and record should be locked
            set ..PersistedTwin.Finalized = 1
            // Ensure the persisted twin is saved (automatically included by saving this temp object)
            set sc = ..%Save()
        }
    }
    quit sc
]]></Implementation>
</Method>

<Method name="%OnClose">
<Description>
Persist this more permanently by saving to the PersistedTwin</Description>
<Private>1</Private>
<ReturnType>%Status</ReturnType>
<ServerOnly>1</ServerOnly>
<Implementation><![CDATA[
    set sc = $$$OK
    if $$$comClassDefined("%IPM.General.History") {
        // Persist to twin if not already done
        set sc = ..PersistToTwin()

        // Delete this temp entry as it's no longer needed
        set sc = $$$ADDSC(sc, ..%DeleteId(..%Id()))
    }
    quit sc
]]></Implementation>
</Method>

<Storage name="Default">
<Type>%Storage.Persistent</Type>
<DataLocation>^IRIS.Temp.HistoryTempD</DataLocation>
<DefaultData>HistoryTempDefaultData</DefaultData>
<IdLocation>^IRIS.Temp.HistoryTempD</IdLocation>
<IndexLocation>^IRIS.Temp.HistoryTempI</IndexLocation>
<StreamLocation>^IRIS.Temp.HistoryTempS</StreamLocation>
<Data name="HistoryTempDefaultData">
<Value name="1">
<Value>%%CLASSNAME</Value>
</Value>
<Value name="2">
<Value>Action</Value>
</Value>
<Value name="3">
<Value>Package</Value>
</Value>
<Value name="4">
<Value>Version</Value>
</Value>
<Value name="5">
<Value>SourceName</Value>
</Value>
<Value name="6">
<Value>SourceMoniker</Value>
</Value>
<Value name="7">
<Value>SourceDetails</Value>
</Value>
<Value name="8">
<Value>TimeStart</Value>
</Value>
<Value name="9">
<Value>TimeEnd</Value>
</Value>
<Value name="10">
<Value>UserName</Value>
</Value>
<Value name="11">
<Value>Success</Value>
</Value>
<Value name="12">
<Value>Committed</Value>
</Value>
<Value name="13">
<Value>CommandString</Value>
</Value>
<Value name="14">
<Value>Phases</Value>
</Value>
<Value name="15">
<Value>PersistedTwin</Value>
</Value>
</Data>
</Storage>
</Class>
</Export>
