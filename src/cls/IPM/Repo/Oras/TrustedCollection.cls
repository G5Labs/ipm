/// Trust data the notary client verified for one collection, kept so later verifications
/// only accept data that chains from the same root and is no older.
/// The first verification of a collection trusts the root the trust server presents, as Docker Content Trust does.
/// After the collection's signer replaces its root key without rotating it, delete the collection's row to trust the new root:
/// <example>
/// &sql(DELETE FROM %IPM_Repo_Oras.TrustedCollection WHERE GUN = 'registry.company.com/packages/ipm/my-module')
/// </example>
Class %IPM.Repo.Oras.TrustedCollection Extends %Persistent
{

/// Globally unique name of the collection, i.e. the repository without a tag, e.g. "registry.company.com/packages/ipm/my-module".
Property GUN As %String(COLLATION = "EXACT", MAXLEN = 2048) [ Required ];

/// The notary client's metadata files for the collection other than timestamp.json, by path relative to its metadata directory, e.g. "root.json".
Property Metadata As array Of %String(MAXLEN = "");

Index GUNIndex On GUN [ Unique ];

/// Writes the metadata files into directory, the collection's metadata directory in a notary trust directory, given with a trailing separator.
Method WriteTo(directory As %String)
{
    set path = ""
    for {
        set content = ..Metadata.GetNext(.path)
        if (path = "") {
            quit
        }
        set filename = directory _ path
        $$$ThrowOnError(##class(%IPM.Utils.File).CreateDirectoryChain(##class(%File).GetDirectory(filename)))
        set filename = ##class(%File).NormalizeFilename(filename)
        set file = ##class(%Stream.FileBinary).%New()
        $$$ThrowOnError(file.LinkToFile(filename))
        $$$ThrowOnError(file.Write(content))
        $$$ThrowOnError(file.%Save())
    }
}

/// Replaces the metadata files with those in directory, other than timestamp.json.
Method ReadFrom(directory As %String)
{
    do ..Metadata.Clear()
    do ..ReadDirectory(##class(%File).NormalizeDirectory(directory), "")
}

Method ReadDirectory(
	directory As %String,
	prefix As %String) [ Private ]
{
    set rs = ##class(%File).FileSetFunc(directory, "*", , 1)
    while rs.%Next() {
        set name = rs.ItemName
        if (rs.Type = "D") {
            do ..ReadDirectory(##class(%File).NormalizeDirectory(rs.Name), prefix _ name _ "/")
            continue
        }
        if (prefix = "") && (name = "timestamp.json") {
            continue
        }
        set file = ##class(%Stream.FileBinary).%New()
        $$$ThrowOnError(file.LinkToFile(rs.Name))
        set content = ""
        while 'file.AtEnd {
            set content = content _ file.Read(, .sc)
            $$$ThrowOnError(sc)
        }
        $$$ThrowOnError(..Metadata.SetAt(content, prefix _ name))
    }
}

Storage Default
{
<Data name="Metadata">
<Attribute>Metadata</Attribute>
<Structure>subnode</Structure>
<Subscript>"Metadata"</Subscript>
</Data>
<Data name="TrustedCollectionDefaultData">
<Value name="1">
<Value>%%CLASSNAME</Value>
</Value>
<Value name="2">
<Value>GUN</Value>
</Value>
</Data>
<DataLocation>^IPM.Repo.Oras.TrustD</DataLocation>
<DefaultData>TrustedCollectionDefaultData</DefaultData>
<IdLocation>^IPM.Repo.Oras.TrustD</IdLocation>
<IndexLocation>^IPM.Repo.Oras.TrustI</IndexLocation>
<StreamLocation>^IPM.Repo.Oras.TrustS</StreamLocation>
<Type>%Storage.Persistent</Type>
}

}
