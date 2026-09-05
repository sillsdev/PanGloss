using System;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace XampleProjector
{
	internal static class Program
	{
		private const string DefaultFieldWorksDir = @"C:\Program Files\SIL\FieldWorks 9";
		private const string FieldWorksDirEnvVar = "PANGLOSS_FIELDWORKS_DIR";

		private static int Main(string[] args)
		{
			if (args.Length == 0)
			{
				WriteUsage();
				return ExitCodes.Usage;
			}

			// --validate-capture is the portable-CI path: it must work with no FieldWorks
			// install present at all, so it is handled before anything below touches FieldWorks.
			if (args[0] == "--validate-capture")
			{
				if (args.Length != 2)
				{
					WriteUsage();
					return ExitCodes.Usage;
				}
				return ValidateCapture.Run(args[1]);
			}

			// An unknown subcommand is a usage error, decided before anything below touches
			// FieldWorks -- a typo in the subcommand must never pay the cost of (or risk a
			// false pin-mismatch report from) probing an install it was never going to use.
			if (args[0] != "inspect" && args[0] != "project")
			{
				WriteUsage();
				return ExitCodes.Usage;
			}

			string fieldWorksDir = ResolveFieldWorksDir();

			// Widen the native search path, and register the assembly probe, BEFORE any code
			// that touches a FieldWorks type runs. Both registrations happen in Main, which
			// itself references no FieldWorks type, so Main's own JIT cannot race the handler.
			if (!NativeMethods.SetDllDirectory(fieldWorksDir))
			{
				var error = Marshal.GetLastWin32Error();
				Console.Error.WriteLine("SetDllDirectory failed for \"{0}\" (Win32 error {1}).", fieldWorksDir, error);
				return ExitCodes.PinMismatch;
			}
			AppDomain.CurrentDomain.AssemblyResolve += (sender, e) => ResolveFieldWorksAssembly(e.Name, fieldWorksDir);

			var mismatches = FieldWorksPins.Verify(fieldWorksDir);
			if (mismatches.Count > 0)
			{
				Console.Error.WriteLine("FieldWorks install at \"{0}\" does not match the pinned versions this tool was verified against:", fieldWorksDir);
				foreach (var mismatch in mismatches)
					Console.Error.WriteLine("  {0}: expected {1}, found {2}", mismatch.FileName, mismatch.Expected, mismatch.Actual);
				return ExitCodes.PinMismatch;
			}

			return Dispatch(args, fieldWorksDir);
		}

		// Kept as its own method, and marked NoInlining, so the JIT compiles the FieldWorks-
		// touching subcommands only after Main has already registered the assembly resolver.
		[MethodImpl(MethodImplOptions.NoInlining)]
		private static int Dispatch(string[] args, string fieldWorksDir)
		{
			switch (args[0])
			{
				case "inspect":
					return InspectCommand.Run(args, fieldWorksDir);
				case "project":
					return ProjectCommand.Run(args, fieldWorksDir);
				default:
					WriteUsage();
					return ExitCodes.Usage;
			}
		}

		private static string ResolveFieldWorksDir()
		{
			var fromEnv = Environment.GetEnvironmentVariable(FieldWorksDirEnvVar);
			return string.IsNullOrEmpty(fromEnv) ? DefaultFieldWorksDir : fromEnv;
		}

		private static Assembly ResolveFieldWorksAssembly(string assemblyFullName, string fieldWorksDir)
		{
			var simpleName = new AssemblyName(assemblyFullName).Name;
			var candidate = Path.Combine(fieldWorksDir, simpleName + ".dll");
			return File.Exists(candidate) ? Assembly.LoadFrom(candidate) : null;
		}

		internal static void WriteUsage()
		{
			Console.WriteLine("XampleProjector -- FieldWorks 9 HC and XAMPLE projection helper");
			Console.WriteLine();
			Console.WriteLine("  inspect --project <path-to-.fwdata> --out <response.json>");
			Console.WriteLine("  project --project <path-to-.fwdata> --out-dir <dir> --database <name>");
			Console.WriteLine("  --validate-capture <response.json>");
			Console.WriteLine();
			Console.WriteLine("FieldWorks install directory: ${0}, default {1}", FieldWorksDirEnvVar, DefaultFieldWorksDir);
			Console.WriteLine("Exit codes: 0 ok, 2 usage, 3 pin mismatch, 4 project open failure, 5 projection failure, 6 capture validation failure.");
		}
	}
}
