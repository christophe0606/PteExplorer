# Generate checked-in opcode names from the upstream Vela sources in research/.
$ErrorActionPreference = 'Stop'
$header = '// Generated from Arm Vela (Apache-2.0). See THIRD_PARTY.md. Do not edit by hand.'
$out = [Collections.Generic.List[string]]::new()
$out.Add($header)
$out.Add('pub fn name(u85: bool, payload: bool, code: u16) -> Option<&''static str> { match (u85,payload,code) {')
$u85=Get-Content research/u85.hpp -Raw
$old=Get-Content research/registers.py -Raw
foreach ($mode in 0,1) {
  $block=[regex]::Match($u85,"(?s)enum class cmd${mode}_opcode.*?\{(.*?)\};").Groups[1].Value
  foreach($m in [regex]::Matches($block,'(NPU_\w+)\s*=\s*(\d+)')) {
    # Vela's opcode enum omits _STEP, while its instruction struct and fields use it.
    $commandName = $m.Groups[1].Value
    if ($commandName -in @('NPU_SET_RESIZE_X','NPU_SET_RESIZE_Y')) { $commandName += '_STEP' }
    $out.Add(('(true,{0},{1}) => Some("{2}"),' -f @('false','true')[$mode],$m.Groups[2].Value,$commandName))
  }
  $block=[regex]::Match($old,"(?s)class cmd${mode}\(Enum\):(.*?)(?=\nclass )").Groups[1].Value
  foreach($m in [regex]::Matches($block,'(NPU_\w+)\s*=\s*(0x[0-9A-Fa-f]+)')) { $out.Add(('(false,{0},{1}) => Some("{2}"),' -f @('false','true')[$mode],$m.Groups[2].Value,$m.Groups[1].Value)) }
}
$out.Add('_=>None } }')
$out.Add('pub fn fields(u85: bool, name: &str) -> &''static [(&''static str,u8,u8)] { match (u85,name) {')
foreach($m in [regex]::Matches($u85,'(?s)struct (npu_\w+)_t\s*\{(.*?)(?=\nstruct |\z)')) {
  $b=$m.Groups[2].Value.Split(@('public:'),[StringSplitOptions]::None)[0]; $shift=0; $fs=@()
  foreach($f in [regex]::Matches($b,'uint(?:32|64)_t\s+(\w+)\s*:\s*(\d+)')) { $n=$f.Groups[1].Value; $width=[int]$f.Groups[2].Value; if($n -notmatch 'reserved|opcode|control'){$fs+=('("{0}",{1},{2})' -f $n,$shift,$width)}; $shift+=$width }
  if($fs.Count){$out.Add(('(true,"{0}") => &[{1}],' -f $m.Groups[1].Value.ToUpper(),($fs -join ',')))}
}
foreach($m in [regex]::Matches($old,'(?s)class (npu_\w+)_t\(Structure\):\s*_fields_\s*=\s*\[(.*?)\]')) {
  $shift=0; $fs=@()
  foreach($f in [regex]::Matches($m.Groups[2].Value,'\("(\w+)", c_uint32, (\d+)\)')) { $n=$f.Groups[1].Value; $width=[int]$f.Groups[2].Value; if($n -notmatch 'must_be|reserved|cmd_code|payload_size'){$fs+=('("{0}",{1},{2})' -f $n,$shift,$width)}; $shift+=$width }
  if($fs.Count){$out.Add(('(false,"{0}") => &[{1}],' -f $m.Groups[1].Value.ToUpper(),($fs -join ',')))}
}
$out.Add('_=>&[] } }')
$out.Add('pub fn symbol(u85: bool, kind: &str, value: u64) -> Option<&''static str> { match (u85,kind,value) {')
foreach($kind in @('elementwise_mode','pooling_mode','activation_type','activation_precision','activation_format','activation_storage','activation_transpose','activation_reverse','dma_region_mode','dma_stride_mode','dma_idx_mode','ifm_upscale_mode','data_format','ofm_precision','ifm_precision','activation_function','activation_clip_range','acc_format','acc_input','acc_output','microblock','kernel_dilation','kernel_decomposition','weight_format','weight_order','weight_sparsity','resize_mode','round_mode_ifm','round_mode_ofm','branch_cond','broadcast_mode')) {
  $block=[regex]::Match($u85,"(?s)enum class ${kind}\s*:.*?\{(.*?)\};").Groups[1].Value
  foreach($m in [regex]::Matches($block,'(\w+)\s*=\s*(\d+)')) {$out.Add(('(true,"{0}",{1}) => Some("{2}"),' -f $kind,$m.Groups[2].Value,$m.Groups[1].Value))}
  $block=[regex]::Match($old,"(?s)class ${kind}\(Enum\):(.*?)(?=\nclass )").Groups[1].Value
  foreach($m in [regex]::Matches($block,'(\w+)\s*=\s*(\d+)')) {$out.Add(('(false,"{0}",{1}) => Some("{2}"),' -f $kind,$m.Groups[2].Value,$m.Groups[1].Value))}
}
$out.Add('_=>None } }')
[IO.File]::WriteAllLines((Join-Path $PWD src/generated/opcodes.rs),$out)
