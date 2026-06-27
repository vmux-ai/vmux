use rustdoc_types::{Function, GenericArg, GenericArgs, Type};

pub fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::ResolvedPath(p) => {
            let mut s = p.path.clone();
            if let Some(args) = p.args.as_deref() {
                s.push_str(&generic_args(args));
            }
            s
        }
        Type::Primitive(p) => p.clone(),
        Type::Generic(g) => g.clone(),
        Type::BorrowedRef {
            lifetime,
            is_mutable,
            type_,
        } => {
            let mut s = String::from("&");
            if let Some(lt) = lifetime {
                s.push_str(lt);
                s.push(' ');
            }
            if *is_mutable {
                s.push_str("mut ");
            }
            s.push_str(&type_to_string(type_));
            s
        }
        Type::Tuple(items) => {
            let inner: Vec<String> = items.iter().map(type_to_string).collect();
            format!("({})", inner.join(", "))
        }
        Type::Slice(inner) => format!("[{}]", type_to_string(inner)),
        Type::Array { type_, len } => format!("[{}; {len}]", type_to_string(type_)),
        Type::RawPointer { is_mutable, type_ } => {
            let kw = if *is_mutable { "*mut " } else { "*const " };
            format!("{kw}{}", type_to_string(type_))
        }
        Type::QualifiedPath { name, .. } => name.clone(),
        Type::ImplTrait(_) => "impl Trait".into(),
        Type::DynTrait(_) => "dyn Trait".into(),
        Type::Infer => "_".into(),
        _ => "_".into(),
    }
}

fn generic_args(args: &GenericArgs) -> String {
    match args {
        GenericArgs::AngleBracketed { args, .. } if !args.is_empty() => {
            let parts: Vec<String> = args
                .iter()
                .filter_map(|a| match a {
                    GenericArg::Type(t) => Some(type_to_string(t)),
                    GenericArg::Lifetime(lt) => Some(lt.clone()),
                    _ => None,
                })
                .collect();
            if parts.is_empty() {
                String::new()
            } else {
                format!("<{}>", parts.join(", "))
            }
        }
        _ => String::new(),
    }
}

pub fn function_signature(name: &str, f: &Function) -> String {
    let inputs: Vec<String> = f
        .sig
        .inputs
        .iter()
        .map(|(arg, ty)| format!("{arg}: {}", type_to_string(ty)))
        .collect();
    let ret = match &f.sig.output {
        Some(t) => format!(" -> {}", type_to_string(t)),
        None => String::new(),
    };
    format!("pub fn {name}({}){ret}", inputs.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustdoc_types::Type;

    #[test]
    fn primitive() {
        assert_eq!(type_to_string(&Type::Primitive("u32".into())), "u32");
    }

    #[test]
    fn reference_mut() {
        let t = Type::BorrowedRef {
            lifetime: None,
            is_mutable: true,
            type_: Box::new(Type::Primitive("u8".into())),
        };
        assert_eq!(type_to_string(&t), "&mut u8");
    }

    #[test]
    fn tuple() {
        let t = Type::Tuple(vec![
            Type::Primitive("u8".into()),
            Type::Primitive("bool".into()),
        ]);
        assert_eq!(type_to_string(&t), "(u8, bool)");
    }
}
