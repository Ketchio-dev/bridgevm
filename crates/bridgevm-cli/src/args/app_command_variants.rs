#[path = "app_operand.rs"]
mod operand;
use operand::reject_option_like_operands;
include!("app_command_variants_v2.rs");
