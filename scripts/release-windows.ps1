#requires -Version 5.1
[CmdletBinding()]
param(
    [string]$OutputDirectory,
    [string]$WixDirectory = $env:AXIACCESS_WIX_DIRECTORY,
    [string]$SigningScript = $env:AXIACCESS_SIGNING_SCRIPT,
    [string]$SigningSettings = $env:AXIACCESS_SIGNING_SETTINGS,
    [string]$SignToolPath = $env:AXIACCESS_SIGNTOOL_PATH,
    [string]$DlibPath = $env:AXIACCESS_DLIB_PATH,
    [string]$CargoPath = $env:AXIACCESS_CARGO_PATH,
    [string]$RustcPath = $env:AXIACCESS_RUSTC_PATH,
    [string]$SourceCommit = $env:AXIACCESS_SOURCE_COMMIT,
    [string]$ReleaseTag = $env:AXIACCESS_RELEASE_TAG
)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
Set-StrictMode -Version Latest
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$started=[DateTime]::UtcNow
if(!$OutputDirectory){$OutputDirectory=Join-Path $root ('dist\windows\'+$started.ToString('yyyyMMddTHHmmssZ'))}
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory|Out-Null
function Write-Json([string]$Path,$Value){[IO.File]::WriteAllText($Path,($Value|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))}
function Hash([string]$Path){return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
$script:NativeStep=0
function Native([string]$File,[string[]]$Arguments){
    $script:NativeStep++
    $command=Get-Command $File -ErrorAction Stop
    $info=New-Object Diagnostics.ProcessStartInfo
    $info.FileName=$command.Source
    $quoted=@(foreach($argument in $Arguments){
        if($argument -notmatch '[\s"]' -and $argument.Length){$argument}
        else{'"'+([regex]::Replace(([regex]::Replace($argument,'(\\*)"','$1$1\"')),'(\\+)$','$1$1'))+'"'}
    })
    $info.Arguments=$quoted -join ' '
    $info.WorkingDirectory=$root;$info.UseShellExecute=$false;$info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    $process=New-Object Diagnostics.Process;$process.StartInfo=$info
    try{
        if(!$process.Start()){throw ('Could not start '+$File)}
        $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $text=$stdout.GetAwaiter().GetResult()+$stderr.GetAwaiter().GetResult()
        $log=Join-Path $OutputDirectory ('native-'+$script:NativeStep+'-'+[IO.Path]::GetFileNameWithoutExtension($File)+'.log')
        [IO.File]::WriteAllText($log,$text,[Text.UTF8Encoding]::new($false))
        if($text){Write-Host $text.Substring([Math]::Max(0,$text.Length-12000))}
        $global:LASTEXITCODE=[int]$process.ExitCode
        if($process.ExitCode -ne 0){throw ('Command failed ('+$process.ExitCode+'): '+$File+'; log: '+$log)}
    }finally{$process.Dispose()}
}
function Sign([string]$File,[string]$Label){
    & $SigningScript -CandidatePath $File -SettingsPath $SigningSettings -StagingRoot (Join-Path $OutputDirectory 'signing-staging') -SignToolPath $SignToolPath -DlibPath $DlibPath -EvidencePath (Join-Path $OutputDirectory ($Label+'-signature.json')) -SourceGitHead $SourceCommit | Out-Host
    if($LASTEXITCODE -ne 0){throw ('Artifact signing failed: '+$Label)}
    $signature=Get-AuthenticodeSignature -LiteralPath $File
    $policy=Get-Content -LiteralPath $SigningSettings -Raw|ConvertFrom-Json
    if($signature.Status -ne 'Valid' -or !$signature.SignerCertificate -or !$signature.TimeStamperCertificate -or $signature.SignerCertificate.Subject -cne $policy.expected_signer_subject){throw ('Signature, timestamp or publisher verification failed: '+$Label)}
    return @{status=[string]$signature.Status;publisher=$signature.SignerCertificate.Subject;thumbprint=$signature.SignerCertificate.Thumbprint;timestampPublisher=$signature.TimeStamperCertificate.Subject}
}
function Read-MsiMetadata([string]$Path){
    $installer=New-Object -ComObject WindowsInstaller.Installer
    $database=$null
    try{
        $database=$installer.OpenDatabase($Path,0)
        $values=[ordered]@{}
        foreach($name in @('ProductName','ProductCode','ProductVersion','AXIACCESS_SOURCE_COMMIT','AXIACCESS_INPUT_DIGEST')){
            $view=$database.OpenView(('SELECT `Value` FROM `Property` WHERE `Property` = '''+$name+''''))
            $record=$null
            try{$view.Execute();$record=$view.Fetch();if(!$record){throw ('MSI identity property missing: '+$name)};$values[$name]=$record.StringData(1)}finally{if($record){[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($record)};$view.Close();[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)}
        }
        $files=@();$view=$database.OpenView('SELECT `File`, `FileName`, `FileSize` FROM `File`')
        try{
            $view.Execute()
            while($record=$view.Fetch()){
                try{$files+=@{id=$record.StringData(1);name=$record.StringData(2);bytes=$record.IntegerData(3)}}finally{[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($record)}
            }
        }finally{$view.Close();[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)}
        $values.files=$files
        return $values
    }finally{if($database){[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($database)};[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($installer)}
}
$receipt=[ordered]@{schemaVersion=1;product='AxiAccess Sender';platform='windows';architecture='x86_64';startedUtc=$started.ToString('o');status='building';daemonChanged=$false;providerStateTouched=$false}
try{
    $ide=Join-Path $env:USERPROFILE 'AxiOwlBrowserWork\checkout'
    if(!$WixDirectory){$WixDirectory=Join-Path $ide 'packaging\windows-ide\cache\msi\wix314'}
    if(!$SigningScript){$SigningScript=@(Get-ChildItem (Join-Path $ide 'packaging\windows-ide\cache\website-publisher\*\installer\windows\invoke-artifact-signing.ps1')|Sort-Object LastWriteTimeUtc -Descending|Select-Object -First 1 -ExpandProperty FullName)[0]}
    if(!$SigningSettings){$SigningSettings=Join-Path $ide 'scripts\windows\artifact-signing.settings.json'}
    $signingTools=Join-Path $env:LOCALAPPDATA 'AxiOwlBuildTools\ArtifactSigningTools'
    if(!$SignToolPath){$SignToolPath=Join-Path $signingTools 'Microsoft.Windows.SDK.BuildTools-10.0.28000.2270\bin\10.0.28000.0\x64\signtool.exe'}
    if(!$DlibPath){$DlibPath=Join-Path $signingTools 'Microsoft.ArtifactSigning.Client-1.0.128\bin\x64\Azure.CodeSigning.Dlib.dll'}
    if(!$CargoPath){$CargoPath=(Get-Command cargo.exe -ErrorAction Stop).Source}
    if(!$RustcPath){$RustcPath=Join-Path (Split-Path $CargoPath -Parent) 'rustc.exe'}
    foreach($file in @((Join-Path $WixDirectory 'candle.exe'),(Join-Path $WixDirectory 'light.exe'),$SigningScript,$SigningSettings,$SignToolPath,$DlibPath,$CargoPath,$RustcPath)){if(!(Test-Path -LiteralPath $file -PathType Leaf) -or (Get-Item -LiteralPath $file).Length -eq 0){throw ('Required release tool is absent or empty: '+$file)}}
    $actualCommit=(& git -C $root rev-parse HEAD).Trim()
    if($LASTEXITCODE -ne 0 -or $actualCommit -notmatch '^[a-f0-9]{40}$'){throw 'Unable to identify AxiAccess source HEAD.'}
    if($SourceCommit -and $SourceCommit -cne $actualCommit){throw 'Requested source commit does not match the checked out source.'}
    $SourceCommit=$actualCommit
    $manifest=Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
    $version=[regex]::Match($manifest,'(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"').Groups[1].Value
    if(!$version){throw 'Cargo package version is not MSI-compatible.'}
    $versionParts=@($version.Split('.')|ForEach-Object{[int]$_})
    if($versionParts[0] -gt 255 -or $versionParts[1] -gt 255 -or $versionParts[2] -gt 65535){throw 'Version exceeds Windows Installer limits.'}
    $sourceFiles=@(Get-ChildItem -LiteralPath $root -File -Recurse|Where-Object{$relative=$_.FullName.Substring($root.Length+1);$relative -notmatch '^(?:\.git|target|dist|output)[\\/]' -and $relative -notmatch '(?:^|[\\/])__pycache__[\\/]'}|Sort-Object FullName|ForEach-Object{@{path=$_.FullName.Substring($root.Length+1).Replace('\','/');sha256=Hash $_.FullName}})
    Write-Json (Join-Path $OutputDirectory 'source-files.json') $sourceFiles
    $sourceDigest=Hash (Join-Path $OutputDirectory 'source-files.json')
    $lockDigest=Hash (Join-Path $root 'Cargo.lock')
    $cargoVersion=(& $CargoPath --version).Trim()
    if($LASTEXITCODE -ne 0 -or !$cargoVersion){throw 'Cargo did not report its identity.'}
    $rustcVersion=(& $RustcPath -Vv|Out-String).Trim()
    if($LASTEXITCODE -ne 0 -or !$rustcVersion){throw 'Rust compiler did not report its identity.'}
    $recipe=[ordered]@{sourceCommit=$SourceCommit;sourceTreeSha256=$sourceDigest;cargoLockSha256=$lockDigest;target='x86_64-pc-windows-msvc';cargo=$cargoVersion;rustc=$rustcVersion;profile='release';rustflags='-C target-feature=+crt-static';entrypointSha256=Hash $PSCommandPath;wixSourceSha256=Hash (Join-Path $root 'packaging\windows\sender.wxs');wixCandleSha256=Hash (Join-Path $WixDirectory 'candle.exe');wixLightSha256=Hash (Join-Path $WixDirectory 'light.exe');sessionHelperSha256=Hash (Join-Path $root 'packaging\windows\session.ps1');signingPolicySha256=Hash $SigningSettings;signingHelperSha256=Hash $SigningScript;signToolSha256=Hash $SignToolPath;signingDlibSha256=Hash $DlibPath}
    Write-Json (Join-Path $OutputDirectory 'build-recipe.json') $recipe
    $inputDigest=Hash (Join-Path $OutputDirectory 'build-recipe.json')
    $cache=Join-Path $root ('target\sender-cache\'+$inputDigest)
    # Cargo owns a reusable compiler cache; completed native payloads are bound
    # separately to the exact commit/lock/toolchain/recipe identity above.
    $cargoTarget=Join-Path $root 'target\windows-msvc-static'
    $env:CARGO_TARGET_DIR=$cargoTarget
    $previousFlags=$env:RUSTFLAGS
    try{
        $env:RUSTFLAGS='-C target-feature=+crt-static'
        $env:AXIACCESS_SOURCE_COMMIT=$SourceCommit
        $env:AXIACCESS_SOURCE_REVISION=$SourceCommit
        $env:RUSTC=$RustcPath
        Push-Location $root
        try{Native $CargoPath @('build','--locked','--release','--target','x86_64-pc-windows-msvc')}finally{Pop-Location}
    }finally{$env:RUSTFLAGS=$previousFlags}
    New-Item -ItemType Directory -Force -Path $cache|Out-Null
    Copy-Item -LiteralPath (Join-Path $cargoTarget 'x86_64-pc-windows-msvc\release\axiaccess.exe') -Destination (Join-Path $cache 'axiaccess.exe')
    Copy-Item -LiteralPath (Join-Path $OutputDirectory 'build-recipe.json') -Destination $cache
    $stage=Join-Path $OutputDirectory 'payload'
    New-Item -ItemType Directory -Force -Path $stage|Out-Null
    Copy-Item -LiteralPath (Join-Path $cache 'axiaccess.exe') -Destination (Join-Path $stage 'axiaccess.exe')
    Copy-Item -LiteralPath (Join-Path $root 'packaging\windows\session.ps1') -Destination $stage
    $nativeSignature=Sign (Join-Path $stage 'axiaccess.exe') 'sender'
    $payload=[ordered]@{schemaVersion=1;product='AxiAccess Sender';version=$version;sourceCommit=$SourceCommit;sourceTreeSha256=$sourceDigest;inputDigest=$inputDigest;architecture='x86_64';minimumOs='Windows 10 or Windows Server 2016 with a graphical user session';minimumOsSource='https://doc.rust-lang.org/rustc/platform-support.html';runtime='native Rust, application-local static CRT';userSessionOnly=$true;observationOnly=$true;defaultListen='127.0.0.1:8765';files=@(@{path='axiaccess.exe';sha256=Hash (Join-Path $stage 'axiaccess.exe');bytes=(Get-Item (Join-Path $stage 'axiaccess.exe')).Length;signature=$nativeSignature},@{path='session.ps1';sha256=Hash (Join-Path $stage 'session.ps1');bytes=(Get-Item (Join-Path $stage 'session.ps1')).Length})}
    Write-Json (Join-Path $stage 'payload-manifest.json') $payload
    $msi=Join-Path $OutputDirectory ('AxiAccess-Sender-Windows-x64-'+$version+'.msi')
    # Stable per-version product identity prevents two registrations when an
    # operator rebuilds the same version. A new release increments Cargo's
    # product version; same-version byte replacement uses normal MSI repair.
    $productHasher=[Security.Cryptography.SHA256]::Create()
    try{$productBytes=$productHasher.ComputeHash([Text.Encoding]::UTF8.GetBytes('morganross/AxiAccess/windows-x64/'+$version))[0..15]}finally{$productHasher.Dispose()}
    $productBytes[6]=($productBytes[6] -band 15) -bor 128
    $productBytes[8]=($productBytes[8] -band 63) -bor 128
    $productCode=[guid]::new([byte[]]$productBytes).ToString('B').ToUpperInvariant()
    $wixObject=Join-Path $OutputDirectory 'sender.wixobj'
    Native (Join-Path $WixDirectory 'candle.exe') @('-nologo','-arch','x64',('-dVersion='+$version),('-dProductCode='+$productCode),('-dSourceCommit='+$SourceCommit),('-dInputDigest='+$inputDigest),('-dStageDir='+$stage),'-out',$wixObject,(Join-Path $root 'packaging\windows\sender.wxs'))
    # ICE91 assumes ALLUSERS can request a machine install. This MSI explicitly
    # rejects ALLUSERS and is intentionally per-user; retain every other ICE.
    Native (Join-Path $WixDirectory 'light.exe') @('-nologo','-wx','-sice:ICE91','-out',$msi,$wixObject)
    $msiMetadata=Read-MsiMetadata $msi
    if($msiMetadata.ProductName -cne 'AxiAccess Sender' -or $msiMetadata.ProductCode -ine $productCode -or $msiMetadata.ProductVersion -cne $version -or $msiMetadata.AXIACCESS_SOURCE_COMMIT -cne $SourceCommit -or $msiMetadata.AXIACCESS_INPUT_DIGEST -cne $inputDigest){throw 'Linked MSI identity/provenance differs from this release operation.'}
    $expectedFiles=@{SenderExe=(Get-Item (Join-Path $stage 'axiaccess.exe')).Length;SessionScript=(Get-Item (Join-Path $stage 'session.ps1')).Length;PayloadManifest=(Get-Item (Join-Path $stage 'payload-manifest.json')).Length}
    if($msiMetadata.files.Count -ne $expectedFiles.Count){throw 'MSI contains an unexpected file set.'}
    foreach($file in $msiMetadata.files){if(!$expectedFiles.ContainsKey($file.id) -or $file.bytes -ne $expectedFiles[$file.id]){throw ('MSI file inventory differs from staged payload: '+$file.id)}}
    Write-Json (Join-Path $OutputDirectory 'msi-metadata.json') $msiMetadata
    $msiSignature=Sign $msi 'installer'
    $checksum=(Hash $msi)+'  '+[IO.Path]::GetFileName($msi)
    [IO.File]::WriteAllText(($msi+'.sha256'),($checksum+"`n"),[Text.UTF8Encoding]::new($false))
    $receipt.status='built-signed';$receipt.sourceCommit=$SourceCommit;$receipt.sourceTreeSha256=$sourceDigest;$receipt.version=$version;$receipt.inputDigest=$inputDigest;$receipt.productCode=$productCode;$receipt.installer=@{path=$msi;sha256=Hash $msi;bytes=(Get-Item $msi).Length;signature=$msiSignature};$receipt.payload=$payload;$receipt.msiMetadata=$msiMetadata;$receipt.installationPerformed=$false
    if($ReleaseTag){
        Native 'gh.exe' @('release','view',$ReleaseTag,'--repo','morganross/AxiAccess')
        Native 'gh.exe' @('release','upload',$ReleaseTag,'--repo','morganross/AxiAccess',$msi,($msi+'.sha256'),(Join-Path $OutputDirectory 'build-recipe.json'),(Join-Path $stage 'payload-manifest.json'))
        $receipt.status='published-signed';$receipt.releaseTag=$ReleaseTag
    }
}catch{$receipt.status='failed';$receipt.error=$_.Exception.Message;throw}finally{
    $receipt.finishedUtc=[DateTime]::UtcNow.ToString('o')
    $receipt.elapsedSeconds=([DateTime]::UtcNow-$started).TotalSeconds
    Write-Json (Join-Path $OutputDirectory 'release-receipt.json') $receipt
}
Write-Output ($receipt|ConvertTo-Json -Depth 12)
