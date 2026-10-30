param(
    [string]$Metadata = "$PSScriptRoot/../research/pytorch-metadata.json",
    [string]$Quantized = "$PSScriptRoot/../research/quantized_functions.yaml",
    [string]$Output = "$PSScriptRoot/../src/generated/operator-schemas.txt"
)
$ErrorActionPreference = 'Stop'
# Netron's MIT-licensed metadata contains resolved ATen schemas, including
# generated .out overloads absent from native_functions.yaml's func entries.
# ExecuTorch's quantization schemas take precedence for its custom operators.
$schemas = [Collections.Generic.SortedDictionary[string,string]]::new([StringComparer]::Ordinal)
foreach ($item in (Get-Content -Raw -LiteralPath $Metadata | ConvertFrom-Json)) {
    $signature = $item.name
    if ($signature -match '^(aten::|quantized_decomposed::)') {
        $schemas[$signature.Substring(0, $signature.IndexOf('('))] = $signature
    }
}
foreach ($line in (Get-Content -LiteralPath $Quantized)) {
    if ($line -match '^- func: (quantized_decomposed::.+)$') {
        $signature = $Matches[1]
        $schemas[$signature.Substring(0, $signature.IndexOf('('))] = $signature
    }
}
[IO.File]::WriteAllText([IO.Path]::GetFullPath($Output), (($schemas.Values -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
Write-Output "Generated $($schemas.Count) operator schemas."
