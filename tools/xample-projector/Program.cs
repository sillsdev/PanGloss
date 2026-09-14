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

			// check-label-uniqueness is likewise portable: it proves an already-computed guid-to-
			// label map is injective (Dictionary<string,string> processing only), so it needs no
			// FieldWorks install either.
			if (args[0] == "check-label-uniqueness")
				return CheckLabelUniquenessCommand.Run(args);

			// An unknown subcommand is a usage error, decided before anything below touches
			// FieldWorks -- a typo in the subcommand must never pay the cost of (or risk a
			// false pin-mismatch report from) probing an install it was never going to use.
			if (args[0] != "inspect" && args[0] != "project" && args[0] != "author" && args[0] != "verify-parity" &&
				args[0] != "mutate" && args[0] != "parse")
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
				case "author":
					return AuthorCommand.Run(args, fieldWorksDir);
				case "verify-parity":
					return VerifyParityCommand.Run(args, fieldWorksDir);
				case "mutate":
					return MutateCommand.Run(args, fieldWorksDir);
				case "parse":
					return ParseCommand.Run(args, fieldWorksDir);
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
			Console.WriteLine("  author --grammar <grammar.xml> --out-dir <dir> --name <ProjectName>");
			Console.WriteLine("         [--vernacular-ws <icu>] [--xample-max-prefixes N] [--xample-max-analyses N]");
			Console.WriteLine("  verify-parity --grammar <grammar.xml> --hc-xml <projected.hc.xml> --guid-map <author-response.json>");
			Console.WriteLine("                --expect WORD=COUNT [--expect WORD=COUNT ...] (at least one required)");
			Console.WriteLine("  mutate --project <path-to-.fwdata> --request <request.json> --out-dir <dir>");
			Console.WriteLine("  parse --project <path-to-.fwdata> --project-dir <dir with <db>adctl.txt/gram.txt/lex.txt>");
			Console.WriteLine("        --database <name> --words <file, one per line> --out <response.json>");
			Console.WriteLine("        [--max-analyses N] [--max-prefixes N] [--max-suffixes N] [--max-infixes N]");
			Console.WriteLine("        [--max-roots N] [--max-interfixes N] [--max-nulls N]");
			Console.WriteLine("  --validate-capture <response.json>");
			Console.WriteLine("  check-label-uniqueness --labels <guid-to-label.json>");
			Console.WriteLine();
			Console.WriteLine("FieldWorks install directory: ${0}, default {1}", FieldWorksDirEnvVar, DefaultFieldWorksDir);
			Console.WriteLine("Exit codes: 0 ok, 2 usage, 3 pin mismatch, 4 project open failure, 5 projection failure, 6 capture validation failure, 7 author refusal, 8 parity mismatch, 9 mutation refusal, 10 mutation integrity failure, 11 parse engine load failure.");
		}
	}
}
