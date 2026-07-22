# Native ABI host smokes

Run both available host toolchains with:

```powershell
./tools/run-native-host-smokes.ps1
```

Or build `pg-ffi` in release mode and run either example directly:

```powershell
cargo build -p pg-ffi --release
python examples/native-abi/python_ctypes_smoke.py target/release/pangloss_ffi.dll tools/fixtures/supplied-lexicon-host-smoke.xml
dotnet restore examples/native-abi/csharp/PanGlossNativeSmoke.csproj --configfile examples/native-abi/NuGet.Config
dotnet build examples/native-abi/csharp/PanGlossNativeSmoke.csproj --configuration Release --no-restore
dotnet run --project examples/native-abi/csharp/PanGlossNativeSmoke.csproj --configuration Release --no-build -- target/release/pangloss_ffi.dll tools/fixtures/supplied-lexicon-host-smoke.xml
```

Use `libpangloss_ffi.so` on Linux or `libpangloss_ffi.dylib` on macOS. The examples intentionally
stay close to the C ABI: they pass length-delimited UTF-8 JSON and free every returned buffer and
handle.
