// Print the decompiled pseudo-C of every function whose name contains one of the arguments (case-sensitive).
//   args: <substring> [<substring> ...]      e.g. PerlinNoise
// Output goes to the console only: never commit it (AGENTS.md section 13).
//@category Townscaper

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;

public class DecompileName extends GhidraScript {
	@Override
	protected void run() throws Exception {
		DecompInterface di = new DecompInterface();
		di.openProgram(currentProgram);
		for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
			for (String want : getScriptArgs()) {
				if (!f.getName(true).contains(want)) {
					continue;
				}
				DecompileResults r = di.decompileFunction(f, 120, monitor);
				println("==== " + f.getEntryPoint() + "  " + f.getName(true));
				println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "decompile failed: " + r.getErrorMessage());
			}
		}
		di.dispose();
	}
}
