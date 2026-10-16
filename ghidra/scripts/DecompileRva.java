// Print the decompiled pseudo-C of the functions at the given RVAs (GameAssembly.dll, image base 0x10000000).
//   args: <rva> [<rva> ...]   e.g. 0x3D88D0
// Output goes to the Ghidra log / console only: never commit it (AGENTS.md section 13).
//@category Townscaper

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;

public class DecompileRva extends GhidraScript {
	@Override
	protected void run() throws Exception {
		DecompInterface di = new DecompInterface();
		di.openProgram(currentProgram);
		Address base = currentProgram.getImageBase();
		for (String arg : getScriptArgs()) {
			long rva = Long.decode(arg);
			Address a = base.add(rva);
			Function f = getFunctionContaining(a);
			if (f == null) {
				println("no function at RVA " + arg);
				continue;
			}
			DecompileResults r = di.decompileFunction(f, 120, monitor);
			println("==== RVA " + arg + "  " + f.getName());
			if (f.getComment() != null) {
				println("// " + f.getComment().replace("\n", "\n// "));
			}
			println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "decompile failed: " + r.getErrorMessage());
		}
		di.dispose();
	}
}
