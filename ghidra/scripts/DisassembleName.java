// Print the instructions of every function whose name contains one of the arguments (console only).
//@category Townscaper

import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;

public class DisassembleName extends GhidraScript {
	@Override
	protected void run() throws Exception {
		for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
			for (String want : getScriptArgs()) {
				if (!f.getName(true).equals(want)) {
					continue;
				}
				println("==== " + f.getEntryPoint() + "  " + f.getName(true));
				for (Instruction i : currentProgram.getListing().getInstructions(f.getBody(), true)) {
					println(i.getAddress() + "  " + i);
				}
			}
		}
	}
}
