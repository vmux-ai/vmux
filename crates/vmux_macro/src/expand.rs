use syn::{Fields, Ident, Type};

pub fn variants_for(field_type: &Ident) -> Option<&'static [&'static str]> {
    match field_type.to_string().as_str() {
        "PaneDirection" => Some(&["Top", "Right", "Bottom", "Left"]),
        _ => None,
    }
}

pub fn format_id_template(template: &str, variant_pascal: &str) -> String {
    template.replace("{dir}", &to_snake_case(variant_pascal))
}

pub fn format_label_template(template: &str, variant_pascal: &str) -> String {
    template
        .replace("{Dir}", variant_pascal)
        .replace("{dir}", &to_snake_case(variant_pascal))
}

pub fn lookup_field_type<'a>(fields: &'a Fields, field_name: &str) -> Option<&'a Ident> {
    let Fields::Named(named) = fields else {
        return None;
    };
    for field in &named.named {
        if field.ident.as_ref().map(|i| i.to_string()).as_deref() == Some(field_name)
            && let Type::Path(type_path) = &field.ty
        {
            return type_path.path.get_ident();
        }
    }
    None
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}
