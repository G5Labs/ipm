<?xml version="1.0" encoding="UTF-8"?>
<Export generator="IRIS" version="26" zv="IRIS for UNIX (Ubuntu Server LTS for ARM64 Containers) 2026.1 (Build 234U)" ts="2026-11-14 17:39:37">
<Class name="%IPM.Utils.LockManager">
<Description><![CDATA[
Utility to make managing locking/unlocking easier in more complex settings.
Usage pattern:
<ol>
<li>Create an instance of this class at the beginning of a method.</li>
<li>Call the Lock() and (optionally) Unlock() methods.</li>
<li>When the object goes out of scope, remaining locks are automatically released with the correct flags.</li>
</ol>]]></Description>
<Super>%RegisteredObject</Super>
<TimeChanged>67888,9993.360248502</TimeChanged>
<TimeCreated>67888,9993.360248502</TimeCreated>

<Property name="Cache">
<Description><![CDATA[
Map of (<global>,<flags>) to lock count]]></Description>
<MultiDimensional>1</MultiDimensional>
<Private>1</Private>
</Property>

<Property name="OIDCache">
<Description><![CDATA[
Map of (<class>,<id>) to lock count]]></Description>
<MultiDimensional>1</MultiDimensional>
<Private>1</Private>
</Property>

<Property name="ImmediateReleaseOnClose">
<Description>
If true, locks are released immediately when the instance of this class goes out of scope.
(If not, standard handling for transactions will apply instead.)</Description>
<Type>%Boolean</Type>
<Private>1</Private>
</Property>

<Method name="%OnNew">
<Description><![CDATA[
Set <var>pImmediateRelease</var> to 0 to keep locks in an instance of this class
until the end of the current transaction even once the instance goes out of scope.]]></Description>
<FormalSpec>pImmediateRelease:%Boolean=1</FormalSpec>
<Private>1</Private>
<ReturnType>%Status</ReturnType>
<ServerOnly>1</ServerOnly>
<Implementation><![CDATA[
    set ..ImmediateReleaseOnClose = pImmediateRelease
    quit $$$OK
]]></Implementation>
</Method>

<Method name="Lock">
<FormalSpec>pGlobalName:%String,pFlags:%String="",pTimeout:%Integer=$$$LockTimeout</FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tSC = $$$OK
    try {
        lock +@pGlobalName#pFlags:pTimeout
        if '$test {
            set tSC = $$$ERROR($$$GeneralError,$$$FormatText("Failed to lock %1",pGlobalName))
            quit
        }

        do $increment(i%Cache(pGlobalName,$case(pFlags,"":$char(0),:pFlags)))
    } catch e {
        set tSC = e.AsStatus()
    }
    quit tSC
]]></Implementation>
</Method>

<Method name="Unlock">
<FormalSpec>pGlobalName:%String,pFlags:%String="",pImmediate:%Boolean=0,pUpdateCache:%Boolean=1</FormalSpec>
<Implementation><![CDATA[
    if (pFlags = $char(0)) {
        set pFlags = ""
    }
    lock -@pGlobalName#pFlags_$select(pImmediate:"I",1:"")
    if pUpdateCache {
        set i%Cache(pGlobalName,$case(pFlags,"":$char(0),:pFlags)) = $get(i%Cache(pGlobalName,$case(pFlags,"":$char(0),:pFlags)),0) - 1
        if (i%Cache(pGlobalName,$case(pFlags,"":$char(0),:pFlags)) <= 0) {
            kill i%Cache(pGlobalName,$case(pFlags,"":$char(0),:pFlags))
        }
    }
]]></Implementation>
</Method>

<Method name="LockClassId">
<FormalSpec>pClassName:%Dictionary.Classname,pID:%String,pShared:%Boolean=0,pTimeout:%Integer</FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tSC = $classmethod(pClassName,"%LockId",pID,pShared,.pTimeout)
    if $$$ISOK(tSC) {
        set tOID = $$$oidCompose(pID,pClassName)
        set i%OIDCache(tOID,pShared) = $get(i%OIDCache(tOID,pShared)) + 1
    }
    quit tSC
]]></Implementation>
</Method>

<Method name="UnlockClassId">
<FormalSpec>pClassName:%Dictionary.Classname,pID:%String,pShared:%Boolean=0,pImmediate:%Boolean=0</FormalSpec>
<Implementation><![CDATA[
    set tOID = $$$oidCompose(pID,pClassName)
    if $data(i%OIDCache(tOID,pShared),tLockCount) {
        do $classmethod(pClassName,"%UnlockId",pID,pShared,pImmediate)
        set i%OIDCache(tOID,pShared) = tLockCount - 1
    }
]]></Implementation>
</Method>

<Method name="UnlockAll">
<FormalSpec>pImmediate:%Boolean=1</FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    set tSC = $$$OK
    try {
        set tGlobalName = ""
        for {
            set tGlobalName = $order(i%Cache(tGlobalName))
            if (tGlobalName = "") {
                quit
            }
            set tFlags = ""
            for {
                set tFlags = $order(i%Cache(tGlobalName,tFlags),1,tLockCount)
                if (tFlags = "") {
                    quit
                }
                for i=1:1:tLockCount {
                    do ..Unlock(tGlobalName,tFlags,pImmediate,0)
                }
            }
        }
        kill i%Cache

        set tOID = ""
        for {
            set tOID = $order(i%OIDCache(tOID))
            if (tOID = "") {
                quit
            }
            set tShared = ""
            for {
                set tShared = $order(i%OIDCache(tOID,tShared),1,tLockCount)
                if (tShared = "") {
                    quit
                }
                set tID = $$$oidPrimary(tOID)
                set tClassName = $$$oidClassName(tOID)
                for i=1:1:tLockCount {
                    do ..UnlockClassId(tClassName,tID,tShared,pImmediate)
                }
            }
        }
        kill i%OIDCache
    } catch e {
        set tSC = e.AsStatus()
    }
    quit tSC
]]></Implementation>
</Method>

<Method name="%OnClose">
<Description>
Performs an immediate unlock</Description>
<Private>1</Private>
<ReturnType>%Status</ReturnType>
<ServerOnly>1</ServerOnly>
<Implementation><![CDATA[    quit ..UnlockAll(..ImmediateReleaseOnClose)
]]></Implementation>
</Method>

<Method name="LockClassesRecursive">
<ClassMethod>1</ClassMethod>
<FormalSpec><![CDATA[&pLockManager=..%New(),&pClasses,&pSharedLockedClasses]]></FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    // Based on LockClassesRecursive^%occLock.
    // This method (and the rest of the class to support it) is really just a workaround for PL 149632
    set tSC = $$$OK
    try {
        set tLockManager = ..%New()
        merge tVisited = pSharedLockedClasses
        set tClassKey = ""
        for {
            set tClassKey = $order(pClasses(tClassKey))
            if (tClassKey = "") {
                quit
            }

            set tClassName = tClassKey
            if ($zconvert($extract(tClassKey,*-3,*),"U") = ".CLS") {
                // This isn't actually a legal classname - must be class with extension included.
                set tClassName = $extract(tClassKey,1,*-4)
            }

            // Based on lockoneclassrecursive^%occLock, but locks each dependency in shared mode.
            // We always lock the top-level class, because it may need to be locked exclusively; pVisited is just for
            // classes locked in shared mode.
            $$$ThrowOnError(tLockManager.LockClassDefinition(tClassName))
            if $data(tVisited(tClassName)) {
                continue
            }

            // It would be nice if there was an API for this that accepted tVisited.
            kill tDependencies
            do getalldependencyclasses^%occDepend(tClassName,.tDependencies,.tVisited,,,,,1)

            set tDependencyKey = "A" //Start at 'A' to skip % classes.
            for {
                set tDependencyKey = $order(tDependencies(tDependencyKey))
                if (tDependencyKey = "") {
                    quit
                }

                $$$ThrowOnError(tLockManager.LockClassDefinition(tDependencyKey,1))
            }
        }

        // If all classes were locked successfully, record that they were,
        // and move ownership of the locks from tLockManager to pLockManager.
        // If an error occurred, any locks taken out will be released when tLockManager
        // goes out of scope.
        merge pSharedLockedClasses = tVisited
        do tLockManager.MergeTo(pLockManager)
    } catch e {
        set tSC = e.AsStatus()
    }
    quit tSC
]]></Implementation>
</Method>

<Method name="LockClassDefinition">
<FormalSpec>pClass:%String,pShared:%Boolean=0,pTimeout:%Integer=0</FormalSpec>
<ReturnType>%Status</ReturnType>
<Implementation><![CDATA[
    // Based on ^%occLock (particularly, the error handling and $$Function^%SYS.* calls are taken from there).
    set tSourceNamespace = $$GetClassLockDatabase^%SYS.SECURITY(pClass,"")
    if tSourceNamespace = "" {
        quit $$$OK
    }
    set tLockGlobal = $name(^|tSourceNamespace|oddDEF(pClass))
    set tSC = ..Lock(tLockGlobal,$select(pShared:"SE",1:"E"),pTimeout)
    if $$$ISERR(tSC) {
        set tEscalate = 0
        set tType = $$FindLockOwner^%SYS.cspServer2(tSourceNamespace,"^oddDEF",pClass,.tJob,.tUser,.tEscalate)
        if (tType = 2) {
            quit $$$ERROR($$$ClassLockTableFull,pClass)
        } elseif (tType = 1) {
            if tEscalate {
                quit $$$ERROR($$$ClassLockEscalate,pClass,tUser,tJob)
            } else {
                quit $$$ERROR($$$CanNotLockRoutineInfo,pClass_".cls",tUser,tJob)
            }
        }
        if pShared {
            quit $$$ERROR($$$ClassSharedLockFailed,pClass)
        } else {
            quit $$$ERROR($$$ClassExclusiveLockFailed,pClass)
        }
    }
    quit tSC
]]></Implementation>
</Method>

<Method name="MergeTo">
<FormalSpec>pTargetLockManager:%IPM.Utils.LockManager</FormalSpec>
<Implementation><![CDATA[
    set tGlobalName = ""
    for {
        set tGlobalName = $order(i%Cache(tGlobalName))
        if (tGlobalName = "") {
            quit
        }

        set tFlags = ""
        for {
            set tFlags = $order(i%Cache(tGlobalName,tFlags),1,tLockCount)
            if (tFlags = "") {
                quit
            }
            if $increment(pTargetLockManager.Cache(tGlobalName,tFlags),tLockCount)
        }
    }

    kill i%Cache

    set tOID = ""
    for {
        set tOID = $order(i%OIDCache(tOID))
        if (tOID = "") {
            quit
        }
        set tShared = ""
        for {
            set tShared = $order(i%OIDCache(tOID,tShared),1,tLockCount)
            if (tShared = "") {
                quit
            }
            if $increment(pTargetLockManager.OIDCache(tOID,tShared),tLockCount)
        }
    }

    kill i%OIDCache
]]></Implementation>
</Method>
</Class>
</Export>
