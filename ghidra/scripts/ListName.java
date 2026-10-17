// Print the entry point and full name of every function whose name contains one of the arguments.
//   args: <substring> [<substring> ...]
//@category Townscaper

import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;

public class ListName extends GhidraScript {
	@Override
	protected void run() throws Exception {
		for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
			for (String want : getScriptArgs()) {
				if (f.getName(true).contains(want)) {
					println("FN " + f.getEntryPoint() + "  " + f.getName(true) + "  " + f.getBody().getNumAddresses());
				}
			}
		}
	}
}
