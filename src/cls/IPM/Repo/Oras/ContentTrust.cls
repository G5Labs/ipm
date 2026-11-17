Include %IPM.Common

/// Gets the targets a Docker Content Trust (Notary) server signs for a collection, verifying the server's trust data with the notary client.
/// As with Docker, only targets signed by the collection's targets/releases or targets role count.
/// <example>
/// set trust = ##class(%IPM.Repo.Oras.ContentTrust).%New("https://notary.company.com")
/// // $listbuild("sha256:<hex>", <size>) of the manifest signed for tag 1.0.0
/// set target = trust.GetTarget("registry.company.com/packages/ipm/my-module", "1.0.0")
/// </example>
Class %IPM.Repo.Oras.ContentTrust Extends %RegisteredObject
{

/// Seconds to wait for another process verifying the same collection.
Parameter LOCKTIMEOUT = 60;

/// URL of the trust server.
Property Server As %String(MAXLEN = 2048);

/// Targets(gun) is set once collection gun is verified, and Targets(gun, tag) = $listbuild(digest, size) for each tag it signs.
/// Kept for this object's lifetime, so repeated lookups while it lives reuse one verification.
Property Targets As %String [ MultiDimensional, Private ];

Method %OnNew(server As %String) As %Status [ Private, ServerOnly = 1 ]
{
    set ..Server = server
    return $$$OK
}

/// Returns $listbuild(digest, size) for the manifest signed for tag in collection gun, e.g. $listbuild("sha256:9f86...", 1234).
/// Throws if the trust data cannot be verified or does not sign tag.
Method GetTarget(
	gun As %String,
	tag As %String) As %List
{
    if '$data(..Targets(gun)) {
        do ..Verify(gun)
    }
    if '$data(..Targets(gun, tag), target) {
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("Trust server %1 has no signed trust data for '%2:%3'.", ..Server, gun, tag)))
    }
    return target
}

/// Verifies the trust server's data for collection gun with the notary client and records the targets it signs.
/// The client starts from a new trust directory holding the metadata trusted before, except timestamp.json.
/// Without a timestamp it cannot fall back to cached metadata when the server is unreachable. The rest still makes it
/// refuse data that does not chain from the trusted root or is older than the data trusted before.
Method Verify(gun As %String) [ Private ]
{
    // Docker requires https too: the first verification of a collection trusts whatever root the server presents
    if ($zconvert($piece(..Server, "://"), "L") '= "https") {
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("Trust server %1 must use https.", ..Server)))
    }
    lock +^IPM.Repo.Oras.TrustD("verify", gun):..#LOCKTIMEOUT
    if '$test {
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("Timed out waiting for another process verifying trust data for '%1'.", gun)))
    }
    set trustDirectory = ""
    try {
        set trustDirectory = ##class(%IPM.Utils.File).CreateTempDirectory()
        set metadataDirectory = ##class(%File).NormalizeDirectory(trustDirectory _ "tuf/" _ gun _ "/metadata/")
        set collection = ##class(%IPM.Repo.Oras.TrustedCollection).GUNIndexOpen(gun)
        if $isobject(collection) {
            do collection.WriteTo(metadataDirectory)
        } else {
            set collection = ##class(%IPM.Repo.Oras.TrustedCollection).%New()
            set collection.GUN = gun
        }

        set output = ..RunNotary(trustDirectory, gun, "list", gun, "-r", "targets/releases", "-r", "targets")
        do ..SignedTargets(output, metadataDirectory, .targets)
        kill ..Targets(gun)
        merge ..Targets(gun) = targets
        set ..Targets(gun) = ""

        do collection.ReadFrom(metadataDirectory)
        $$$ThrowOnError(collection.%Save())
    } catch e {
        set exception = e
    }
    lock -^IPM.Repo.Oras.TrustD("verify", gun)
    if (trustDirectory '= "") {
        do ##class(%IPM.Utils.File).RemoveDirectoryTree(trustDirectory)
    }
    if $isobject($get(exception)) {
        throw exception
    }
}

/// Sets targets(name) = $listbuild(digest, size) for each target that both the output of "notary list" and the
/// metadata files the client verified into metadataDirectory give as signed by the targets/releases delegation or the targets role.
/// The client prints target names unescaped, so a row of its output only counts when the verified files sign the same target.
ClassMethod SignedTargets(
	listOutput As %String,
	metadataDirectory As %String,
	Output targets)
{
    kill targets
    do ..ParseTargets(listOutput, .listed)
    do ..ReadTargets(metadataDirectory, .signed)
    set name = ""
    for {
        set name = $order(listed(name), 1, row)
        quit:name=""
        if ($get(signed(name)) = row) {
            set targets(name) = $list(row, 1, 2)
        }
    }
}

/// Sets targets(name) = $listbuild(digest, size, role) for each row of the output of "notary list" whose role is the
/// targets/releases delegation or the targets role. The client lists a target signed by another delegation when neither
/// of those signs its name, and Docker Content Trust does not accept such a target. A name listed twice is left out.
/// <example>
/// do ##class(%IPM.Repo.Oras.ContentTrust).ParseTargets("1.0.0  " _ $translate($justify("", 64), " ", "a") _ "  1004  targets", .targets)
/// // $listbuild("sha256:aaa...", 1004, "targets")
/// zwrite targets("1.0.0")
/// </example>
ClassMethod ParseTargets(
	text As %String,
	Output targets)
{
    kill targets
    for i = 1:1:$length(text, $char(10)) {
        // Columns: NAME DIGEST SIZE ROLE, padded with spaces
        set fields = $listfromstring($zstrip($translate($piece(text, $char(10), i), $char(9,13), "  "), "<=>W"), " ")
        if $listlength(fields) '= 4 {
            continue
        }
        set $listbuild(name, digest, size, role) = fields
        if $increment(rows(name)) > 1 {
            continue
        }
        if '$match(digest, "[0-9a-f]{64}") || '$match(size, "[0-9]{1,15}") {
            continue
        }
        if (role '= "targets/releases") && (role '= "targets") {
            continue
        }
        set targets(name) = $listbuild("sha256:" _ digest, +size, role)
    }
    set name = ""
    for {
        set name = $order(rows(name), 1, count)
        quit:name=""
        if (count > 1) {
            kill targets(name)
        }
    }
}

/// Sets targets(name) = $listbuild(digest, size, role) for each target that targets/releases.json or targets.json in
/// metadataDirectory signs. targets/releases takes precedence, for the names its delegation's paths cover.
/// A file only counts if it matches the hash and length its parent gives, from timestamp.json down, so a file left
/// from an earlier verification is not taken for one the client has just verified.
ClassMethod ReadTargets(
	metadataDirectory As %String,
	Output targets)
{
    kill targets
    set timestamp = ..ReadRoleFile(metadataDirectory, "timestamp", "")
    set snapshot = ..ReadRoleFile(metadataDirectory, "snapshot", timestamp)
    set targetsRole = ..ReadRoleFile(metadataDirectory, "targets", snapshot)
    if '$isobject(targetsRole) {
        $$$ThrowStatus($$$ERROR($$$GeneralError, "The notary client left no verified targets metadata."))
    }
    set releases = ..ReadRoleFile(metadataDirectory, "targets/releases", snapshot)
    if $isobject(releases) {
        do ..AddSignedTargets(releases, "targets/releases", ..DelegationPaths(targetsRole, "targets/releases"), .targets)
    }
    do ..AddSignedTargets(targetsRole, "targets", $listbuild(""), .targets)
}

/// Returns the metadata file of role in metadataDirectory, or "" if it is missing or does not match the hash and length that the
/// metadata of its parent role, as returned by this method, gives for it. With no parent, the file is returned as it is.
ClassMethod ReadRoleFile(
	metadataDirectory As %String,
	role As %String,
	parent As %DynamicObject) As %DynamicObject [ Private ]
{
    set filename = metadataDirectory _ role _ ".json"
    if ((parent = "") && (role '= "timestamp")) || '##class(%File).Exists(filename) {
        return ""
    }
    set file = ##class(%Stream.FileBinary).%New()
    $$$ThrowOnError(file.LinkToFile(filename))
    set content = ""
    while 'file.AtEnd {
        set content = content _ file.Read(, .sc)
        $$$ThrowOnError(sc)
    }
    if $isobject(parent) {
        set meta = parent.%Get("signed").%Get("meta")
        set entry = $select($isobject(meta): meta.%Get(role), 1: "")
        set hashes = $select($isobject(entry): entry.%Get("hashes"), 1: "")
        if '$isobject(hashes) || (entry.%Get("length") '= $length(content)) {
            return ""
        }
        if (hashes.%Get("sha256") '= $system.Encryption.Base64Encode($system.Encryption.SHAHash(256, content), 1)) {
            return ""
        }
    }
    return {}.%FromJSON($zconvert(content, "I", "UTF8"))
}

/// Returns the paths of delegation in the targets metadata targetsRole, or "" if it has no such delegation.
ClassMethod DelegationPaths(
	targetsRole As %DynamicObject,
	delegation As %String) As %List [ Private ]
{
    set delegations = targetsRole.%Get("signed").%Get("delegations")
    set roles = $select($isobject(delegations): delegations.%Get("roles"), 1: "")
    if '$isobject(roles) {
        return ""
    }
    set iterator = roles.%GetIterator()
    while iterator.%GetNext(, .role) {
        if '$isobject(role) || (role.%Get("name") '= delegation) || '$isobject(role.%Get("paths")) {
            continue
        }
        set paths = ""
        set pathIterator = role.%Get("paths").%GetIterator()
        while pathIterator.%GetNext(, .path) {
            set paths = paths _ $listbuild(path)
        }
        return paths
    }
    return ""
}

/// Adds the targets that roleFile, the metadata of role, signs to targets, for names one of paths starts and that targets does not have yet.
ClassMethod AddSignedTargets(
	roleFile As %DynamicObject,
	role As %String,
	paths As %List,
	ByRef targets) [ Private ]
{
    set signed = roleFile.%Get("signed").%Get("targets")
    if '$isobject(signed) {
        return
    }
    set iterator = signed.%GetIterator()
    while iterator.%GetNext(.name, .target) {
        if $data(targets(name)) || '$isobject(target) || (target.%GetTypeOf("length") '= "number") {
            continue
        }
        set covered = 0
        set pointer = 0
        while 'covered && $listnext(paths, pointer, path) {
            set covered = ($extract(name, 1, $length(path)) = path)
        }
        set hashes = target.%Get("hashes")
        if 'covered || '$isobject(hashes) {
            continue
        }
        set digest = $zconvert(##class(%xsd.hexBinary).LogicalToXSD($system.Encryption.Base64Decode(hashes.%Get("sha256"))), "L")
        if $match(digest, "[0-9a-f]{64}") {
            set targets(name) = $listbuild("sha256:" _ digest, target.%Get("length"), role)
        }
    }
}

/// Returns the path of the notary client: the NotaryPath setting, or notary on the PATH.
ClassMethod NotaryExecutable() As %String
{
    set path = ##class(%IPM.Repo.UniversalSettings).GetValue(##class(%IPM.Repo.UniversalSettings).#NotaryPath)
    return $select(path = "": "notary", 1: path)
}

/// Runs the notary client against Server with trustDirectory as its trust directory, and returns its standard output.
/// gun is the collection the command is about, for errors.
/// The client gets an empty configuration file, so a configuration in the user's home directory does not change its checks.
Method RunNotary(
	trustDirectory As %String,
	gun As %String,
	args... As %String) As %String [ Private ]
{
    set configFile = trustDirectory _ "config.json"
    set outFile = trustDirectory _ "stdout.txt"
    set errFile = trustDirectory _ "stderr.txt"
    do ..WriteFile(configFile, "{}")

    set cmdArgs = 0
    for arg = "-c", configFile, "-s", ..Server, "-d", trustDirectory {
        set cmdArgs($increment(cmdArgs)) = arg
    }
    for i = 1:1:$get(args) {
        set cmdArgs($increment(cmdArgs)) = args(i)
    }
    set executable = ..NotaryExecutable()
    try {
        set exitCode = $zf(-100, "/STDOUT=""" _ outFile _ """ /STDERR=""" _ errFile _ """", executable, cmdArgs...)
    } catch e {
        if e.%IsA("%Exception.SystemException") && (e.Name '= "<NOTOPEN>") {
            throw e
        }
        set msg = "Could not run the notary client '%1' to verify '%2' against trust server %3. Install the notary client, or set its path with 'config set NotaryPath <path>'."
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText(msg, executable, gun, ..Server)))
    }
    set output = ..ReadFile(outFile)
    if (exitCode '= 0) {
        // The client reports fatal errors on standard output and logs on standard error
        set details = ..NotaryError(output _ $char(10) _ ..ReadFile(errFile))
        if (details = "") {
            set details = "the notary client '" _ executable _ "' exited with code " _ exitCode
        }
        $$$ThrowStatus($$$ERROR($$$GeneralError, $$$FormatText("Could not verify trust data for '%1' from trust server %2: %3", gun, ..Server, details)))
    }
    return output
}

/// Returns the message of the notary client's fatal error in text on one line, or the last line of text if it reports none.
/// e.g. "* fatal: client is offline" gives "client is offline"
ClassMethod NotaryError(text As %String) As %String
{
    set message = ""
    for i = 1:1:$length(text, $char(10)) {
        set line = $zstrip($translate($piece(text, $char(10), i), $char(9,13), "  "), "<=>W")
        if (line = "") {
            continue
        }
        if (line [ "fatal: ") {
            return $zstrip($piece(line, "fatal: ", 2, *), "<>W")
        }
        set message = line
    }
    return message
}

ClassMethod WriteFile(
	filename As %String,
	content As %String) [ Private ]
{
    set file = ##class(%Stream.FileBinary).%New()
    $$$ThrowOnError(file.LinkToFile(filename))
    $$$ThrowOnError(file.Write(content))
    $$$ThrowOnError(file.%Save())
}

ClassMethod ReadFile(filename As %String) As %String [ Private ]
{
    if '##class(%File).Exists(filename) {
        return ""
    }
    set file = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(file.LinkToFile(filename))
    set content = ""
    while 'file.AtEnd {
        set content = content _ file.Read(, .sc)
        $$$ThrowOnError(sc)
    }
    return content
}

}
