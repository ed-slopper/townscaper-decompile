# Serialized field layouts of the scripts the build uses, from Cpp2IL's empty-body DLLs (row F8g).
#   powershell -NoProfile -File tools/layouts.ps1 [work/cpp2il-dll] [work/scripts.txt] [townscaper-rs/data/layouts.json]
# work/scripts.txt comes from `cargo run -p ts_data --example list_scripts`.
#
# Windows PowerShell 5.1 (.NET Framework): the assemblies are loaded reflection-only (Il2CppDumper's DummyDll
# fails there: its explicit-layout structs have no offsets), which keeps the
# field flags il2cpp's metadata has (NotSerialized, InitOnly, Literal) and [SerializeField] / [Serializable].
# Unity 2020.3's rules: a field is serialized when it is not static/const/readonly/NotSerialized and is public
# or [SerializeField]; its type must be serializable (primitive, string, enum, UnityEngine.Object reference,
# Unity's built-in structs, a [Serializable] class/struct, or a T[] / List<T> of those). Base class first.
# Output: names and types only (no code), read by ts_data::layout.
param(
    [string]$DllDir = "work/cpp2il-dll-attr",
    [string]$Scripts = "work/scripts.txt",
    [string]$Out = "townscaper-rs/data/layouts.json",
    [string]$Dump = "work/il2cppdumper/dump.cs"
)
$ErrorActionPreference = "Stop"
$DllDir = (Resolve-Path $DllDir).Path

[System.AppDomain]::CurrentDomain.add_ReflectionOnlyAssemblyResolve({
    param($s, $e)
    $name = (New-Object System.Reflection.AssemblyName($e.Name)).Name
    $p = Join-Path $DllDir "$name.dll"
    if ((Test-Path $p) -and $name -ne "mscorlib") { return [System.Reflection.Assembly]::ReflectionOnlyLoadFrom($p) }
    return [System.Reflection.Assembly]::ReflectionOnlyLoad($e.Name)
})

$asms = @{}
foreach ($f in Get-ChildItem $DllDir -Filter *.dll) {
    if ($f.BaseName -eq "mscorlib") { continue }
    try { $asms[$f.BaseName] = [System.Reflection.Assembly]::ReflectionOnlyLoadFrom($f.FullName) } catch { }
}

$builtin = @{
    "UnityEngine.Vector2" = "Vector2"; "UnityEngine.Vector3" = "Vector3"; "UnityEngine.Vector4" = "Vector4"
    "UnityEngine.Vector2Int" = "Vector2Int"; "UnityEngine.Vector3Int" = "Vector3Int"
    "UnityEngine.Quaternion" = "Quaternion"; "UnityEngine.Color" = "Color"; "UnityEngine.Color32" = "Color32"
    "UnityEngine.Rect" = "Rect"; "UnityEngine.RectInt" = "RectInt"; "UnityEngine.Bounds" = "Bounds"
    "UnityEngine.BoundsInt" = "BoundsInt"; "UnityEngine.Matrix4x4" = "Matrix4x4"; "UnityEngine.LayerMask" = "LayerMask"
    "UnityEngine.AnimationCurve" = "AnimationCurve"; "UnityEngine.Gradient" = "Gradient"; "UnityEngine.RectOffset" = "RectOffset"
    "UnityEngine.GUIStyle" = "GUIStyle"; "UnityEngine.Hash128" = "Hash128"
}
$prims = @{
    "System.Boolean" = "bool"; "System.SByte" = "i8"; "System.Byte" = "u8"; "System.Int16" = "i16"; "System.UInt16" = "u16"
    "System.Char" = "u16"; "System.Int32" = "i32"; "System.UInt32" = "u32"; "System.Int64" = "i64"; "System.UInt64" = "u64"
    "System.Single" = "f32"; "System.Double" = "f64"; "System.String" = "string"
}

function Test-Derives($t, [string]$name) {
    $b = $t
    while ($b -ne $null) { if ($b.FullName -eq $name) { return $true }; $b = $b.BaseType }
    return $false
}

function Get-Attr($member, [string]$name) {
    foreach ($a in [System.Reflection.CustomAttributeData]::GetCustomAttributes($member)) {
        if ($a.AttributeType.Name -eq $name) { return $true }
    }
    return $false
}

$types = [ordered]@{}
$queue = New-Object System.Collections.Queue

function Get-Key($t) { return $t.ToString() }

# The serialized form of a field type, or $null when Unity would not serialize it.
function Get-TypeRef($t, [bool]$inCollection) {
    if ($t.IsArray) {
        if ($inCollection -or $t.GetArrayRank() -ne 1) { return $null }
        $e = Get-TypeRef $t.GetElementType() $true
        if ($e -eq $null) { return $null }
        return "array:$e"
    }
    if ($t.IsGenericType -and $t.GetGenericTypeDefinition().FullName -eq "System.Collections.Generic.List``1") {
        if ($inCollection) { return $null }
        $e = Get-TypeRef ($t.GetGenericArguments()[0]) $true
        if ($e -eq $null) { return $null }
        return "array:$e"
    }
    $fn = $t.FullName
    if ($fn -ne $null -and $prims.ContainsKey($fn)) { return $prims[$fn] }
    if ($fn -ne $null -and $builtin.ContainsKey($fn)) { return "builtin:" + $builtin[$fn] }
    if ($t.IsEnum) {
        $u = [System.Enum]::GetUnderlyingType($t).FullName
        if ($u -eq $null) { $u = "System.Int32" }
        return "enum:" + $prims[$u]
    }
    if (Test-Derives $t "UnityEngine.Object") { return "pptr" }
    if ($t.IsPrimitive -or $t.IsPointer -or $t.IsInterface -or $t.IsAbstract -or $t.IsGenericParameter) { return $null }
    if ($fn -eq "System.Object" -or (Test-Derives $t "System.Delegate")) { return $null }
    if (-not $t.IsSerializable) { return $null }
    $k = Get-Key $t
    if (-not $types.Contains($k)) { $types[$k] = $null; $queue.Enqueue($t) }
    return "type:$k"
}

function Get-Fields($t) {
    $out = New-Object System.Collections.ArrayList
    $chain = New-Object System.Collections.ArrayList
    $b = $t
    while ($b -ne $null -and $b.FullName -notin @("UnityEngine.MonoBehaviour", "UnityEngine.ScriptableObject", "UnityEngine.Behaviour", "UnityEngine.Component", "UnityEngine.Object", "System.Object", "System.ValueType")) {
        [void]$chain.Insert(0, $b); $b = $b.BaseType
    }
    $flags = [System.Reflection.BindingFlags]"Instance,Public,NonPublic,DeclaredOnly"
    foreach ($c in $chain) {
        foreach ($f in ($c.GetFields($flags) | Sort-Object MetadataToken)) {
            $a = $f.Attributes
            if (($a -band [System.Reflection.FieldAttributes]::Static) -or ($a -band [System.Reflection.FieldAttributes]::Literal) -or
                ($a -band [System.Reflection.FieldAttributes]::InitOnly) -or ($a -band [System.Reflection.FieldAttributes]::NotSerialized)) { continue }
            $public = ($a -band [System.Reflection.FieldAttributes]::FieldAccessMask) -eq [System.Reflection.FieldAttributes]::Public
            $sref = Get-Attr $f "SerializeReference"
            if (-not $public -and -not (Get-Attr $f "SerializeField") -and -not $sref) { continue }
            if ($sref) {
                # [SerializeReference] on a UnityEngine.Object type is stored as a plain object reference
                # (GenericMenuNavigator.selectorTransform: proven by the exact-size test, 12 bytes = one PPtr)
                if (Test-Derives $f.FieldType "UnityEngine.Object") { [void]$out.Add([ordered]@{ n = $f.Name; t = "pptr" }); continue }
                [void]$out.Add([ordered]@{ n = $f.Name; t = "managedref" }); continue
            }
            $r = Get-TypeRef $f.FieldType $false
            if ($r -ne $null) { [void]$out.Add([ordered]@{ n = $f.Name; t = $r }) }
        }
    }
    return ,$out
}

# Fallback for the few types reflection refuses to load (Rewired's custom calculations: an override without a
# body fails the load-time check): base classes through reflection, the type's own fields from dump.cs.
$dumpLines = $null
$csPrims = @{ "bool" = "bool"; "sbyte" = "i8"; "byte" = "u8"; "short" = "i16"; "ushort" = "u16"; "char" = "u16"
    "int" = "i32"; "uint" = "u32"; "long" = "i64"; "ulong" = "u64"; "float" = "f32"; "double" = "f64"; "string" = "string" }
function Get-FieldsFallback($asmObj, [string]$ns, [string]$cls) {
    if ($script:dumpLines -eq $null) { $script:dumpLines = [System.IO.File]::ReadAllLines((Resolve-Path $Dump).Path) }
    $L = $script:dumpLines
    $start = -1
    for ($i = 1; $i -lt $L.Length; $i++) {
        if ($L[$i] -match "^(public|internal).* class $([regex]::Escape($cls)) " -and $L[$i - 1 - [int]($L[$i - 1] -like "[[]*")] -match "Namespace: $([regex]::Escape($ns))$") { $start = $i; break }
    }
    if ($start -lt 0) { throw "not in dump.cs" }
    $baseName = ([regex]::Match($L[$start], ": ([\w\.]+)")).Groups[1].Value
    $out = New-Object System.Collections.ArrayList
    $base = $asmObj.GetType("$ns.$baseName", $false)
    if ($base -ne $null) { foreach ($f in (Get-Fields $base)) { [void]$out.Add($f) } }
    $attrs = @()
    for ($i = $start + 2; $i -lt $L.Length -and $L[$i] -notmatch "^	// (Properties|Methods)|^}"; $i++) {
        $line = $L[$i].Trim()
        if ($line -like "[[]*") { $attrs += $line; continue }
        $m = [regex]::Match($line, "^(public|private|protected|internal)((?: static| readonly| const)*) ([\w\.<>\[\]]+) (\w+)( =.*)?;")
        if (-not $m.Success) { $attrs = @(); continue }
        $isSerField = ($attrs | Where-Object { $_ -like "[[]SerializeField]*" }).Count -gt 0
        $attrs = @()
        if ($m.Groups[2].Value -ne "") { continue }
        if ($m.Groups[1].Value -ne "public" -and -not $isSerField) { continue }
        $tn = $m.Groups[3].Value
        if ($csPrims.ContainsKey($tn)) { [void]$out.Add([ordered]@{ n = $m.Groups[4].Value; t = $csPrims[$tn] }); continue }
        $nested = $null
        try { $nested = $asmObj.GetType("$ns." + ($tn -replace "\.", "+"), $false) } catch { }
        if ($nested -eq $null) {
            # unloadable too (nested in the failing type): an enum in dump.cs is serialized as an int either way
            if (($L | Select-String -SimpleMatch "enum $tn " -Quiet)) { [void]$out.Add([ordered]@{ n = $m.Groups[4].Value; t = "enum:i32" }); continue }
            throw "field type $tn"
        }
        $r = Get-TypeRef $nested $false
        if ($r -ne $null) { [void]$out.Add([ordered]@{ n = $m.Groups[4].Value; t = $r }) }
    }
    return ,$out
}

$found = [ordered]@{}
$missing = New-Object System.Collections.ArrayList
foreach ($line in Get-Content $Scripts) {
    $asm, $ns, $cls = $line.Split("|")
    $full = if ($ns) { "$ns.$cls" } else { $cls }
    $t = $null
    try {
        if ($asms.ContainsKey($asm)) { $t = $asms[$asm].GetType($full, $false) }
        if ($t -eq $null) { [void]$missing.Add("$line (not found)"); continue }
        $found[$line] = [ordered]@{ fields = (Get-Fields $t) }
    } catch {
        try { $found[$line] = [ordered]@{ fields = (Get-FieldsFallback $asms[$asm] $ns $cls); from = "dump.cs" } }
        catch { [void]$missing.Add("$line ($($_.Exception.Message))") }
    }
}
while ($queue.Count -gt 0) {
    $t = $queue.Dequeue()
    try { $types[(Get-Key $t)] = [ordered]@{ fields = (Get-Fields $t) } }
    catch { [void]$missing.Add("type $(Get-Key $t) ($($_.Exception.Message))") }
}

$doc = [ordered]@{
    note = "Serialized field layouts (names and types only) generated by tools/layouts.ps1 from the reference build; see AGENTS.md section 2"
    scripts = $found
    types = $types
    missing = $missing
}
$json = $doc | ConvertTo-Json -Depth 12 -Compress
[System.IO.File]::WriteAllText((Join-Path (Get-Location) $Out), $json, (New-Object System.Text.UTF8Encoding($false)))
Write-Output ("layouts: {0} scripts, {1} types, {2} missing" -f $found.Count, $types.Count, $missing.Count)
