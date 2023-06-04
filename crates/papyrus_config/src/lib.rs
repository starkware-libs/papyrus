pub type ParamPath = String;
pub type SerializedValue = String;
pub type Description = String;

pub const DEFAULT_CHAIN_ID: &str = "SN_MAIN";

pub trait SubConfig
where
    Self: Sized,
{
    fn config_name() -> String;

    fn param_path(param_name: String) -> ParamPath {
        let config_name = Self::config_name();
        format!("{config_name}.{param_name}")
    }

    fn dump(&self) -> Vec<(ParamPath, Description, SerializedValue)>;
}
