import { z } from "zod";

/** Rust's unit `()` - a command that answers nothing - arrives as JSON null,
 *  which callers see as `void`. */
export const NO_REPLY = z.null().transform(() => undefined);
