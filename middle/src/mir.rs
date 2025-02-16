use super::*;
use std::{
    cell::Cell,
    fmt,
};

pub type MirMeta = Repr;
pub type MirNode<T> = Node<T, MirMeta>;

// TODO: Keep track of scope, perhaps?
#[derive(Copy, Clone, Debug)]
pub struct LocalId(usize);

#[derive(Clone, Debug, PartialEq)]
pub enum Const<U> {
    Unknown(U), // Value not currently known
    Nat(u64),
    Int(i64),
    Real(f64),
    Char(char),
    Bool(bool),
    Tuple(Vec<Self>),
    List(Vec<Self>),
    Sum(usize, Box<Self>),
    Union(u64, Box<Self>),
}

pub type Literal = Const<!>;
pub type Partial = Const<Option<Local>>;

impl<U: fmt::Debug + Clone> Const<U> {
    pub fn nat(&self) -> u64 { if let Const::Nat(x) = self { *x } else { panic!("{:?}", self) } }
    pub fn int(&self) -> i64 { if let Const::Int(x) = self { *x } else { panic!("{:?}", self) } }
    pub fn bool(&self) -> bool { if let Const::Bool(x) = self { *x } else { panic!("{:?}", self) } }
    pub fn list(&self) -> Vec<Self> { if let Const::List(x) = self { x.clone() } else { panic!("{:?}", self) } }
}

impl Partial {
    pub fn to_literal(&self) -> Option<Literal> {
        match self {
            Self::Unknown(_) => None,
            Self::Nat(x) => Some(Literal::Nat(*x)),
            Self::Int(x) => Some(Literal::Int(*x)),
            Self::Real(x) => Some(Literal::Real(*x)),
            Self::Char(c) => Some(Literal::Char(*c)),
            Self::Bool(x) => Some(Literal::Bool(*x)),
            Self::Tuple(fields) => Some(Literal::Tuple(fields
                .iter()
                .map(|field| field.to_literal())
                .collect::<Option<_>>()?)),
            Self::List(items) => Some(Literal::List(items
                .iter()
                .map(|item| item.to_literal())
                .collect::<Option<_>>()?)),
            Self::Sum(v, inner) => Some(Literal::Sum(*v, Box::new(inner.to_literal()?))),
            Self::Union(ty, inner) => Some(Literal::Union(*ty, Box::new(inner.to_literal()?))),
        }
    }
}

impl Literal {
    pub fn to_partial(&self) -> Partial {
        match self {
            Self::Unknown(x) => Partial::Unknown(*x),
            Self::Nat(x) => Partial::Nat(*x),
            Self::Int(x) => Partial::Int(*x),
            Self::Real(x) => Partial::Real(*x),
            Self::Char(c) => Partial::Char(*c),
            Self::Bool(x) => Partial::Bool(*x),
            Self::Tuple(fields) => Partial::Tuple(fields
                .iter()
                .map(|field| field.to_partial())
                .collect()),
            Self::List(items) => Partial::List(items
                .iter()
                .map(|item| item.to_partial())
                .collect()),
            Self::Sum(v, inner) => Partial::Sum(*v, Box::new(inner.to_partial())),
            Self::Union(ty, inner) => Partial::Union(*ty, Box::new(inner.to_partial())),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Intrinsic {
    MakeList(Repr),
    NotBool,
    NegNat,
    NegInt,
    NegReal,
    AddNat,
    AddInt,
    SubNat,
    SubInt,
    MulNat,
    MulInt,
    EqNat,
    EqInt,
    EqChar,
    NotEqNat,
    NotEqInt,
    NotEqChar,
    LessNat,
    LessInt,
    MoreNat,
    MoreInt,
    LessEqNat,
    LessEqInt,
    MoreEqNat,
    MoreEqInt,
    Join(Repr),
    Union(u64), // Type ID
}

#[derive(Clone, Debug)]
pub enum Pat {
    Wildcard,
    Literal(Literal), // Expression is evaluated and then compared
    Single(MirNode<Binding>),
    Add(MirNode<Binding>, u64),
    Tuple(Vec<MirNode<Binding>>),
    ListExact(Vec<MirNode<Binding>>),
    ListFront(Vec<MirNode<Binding>>, Option<MirNode<Binding>>),
    Variant(usize, MirNode<Binding>),
    UnionVariant(u64, MirNode<Binding>),
}

// Uniquely refer to locals *without* shadowing
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Local(pub usize);

impl Local {
    pub fn new() -> Self {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static ID: AtomicUsize = AtomicUsize::new(0);
        Self(ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub pat: Pat,
    pub name: Option<Local>,
}

impl Binding {
    pub fn wildcard(name: impl Into<Option<Local>>) -> Self {
        Self {
            pat: Pat::Wildcard,
            name: name.into(),
        }
    }

    pub fn is_refutable(&self) -> bool {
        match &self.pat {
            Pat::Wildcard => false,
            Pat::Literal(c) => match c {
                Const::Tuple(fields) if fields.is_empty() => false,
                _ => true,
            },
            Pat::Single(inner) => inner.is_refutable(),
            Pat::Add(lhs, rhs) => *rhs > 0 || lhs.is_refutable(),
            Pat::Tuple(fields) => fields
                .iter()
                .any(|field| field.is_refutable()),
            Pat::ListExact(_) => true,
            Pat::ListFront(items, tail) => items.len() > 0 || tail.as_ref().map_or(false, |tail| tail.is_refutable()),
            Pat::Variant(_, _) => true, // TODO: Check number of variants
            Pat::UnionVariant(_, _) => true, // TODO: Check number of variants
        }
    }

    fn visit_bindings(self: &MirNode<Self>, mut bind: &mut impl FnMut(Local, &Repr)) {
        self.name.map(|name| bind(name, self.meta()));
        match &self.pat {
            Pat::Wildcard => {},
            Pat::Literal(_) => {},
            Pat::Single(inner) => inner.visit_bindings(bind),
            Pat::Add(lhs, _) => lhs.visit_bindings(bind),
            Pat::Tuple(fields) => fields
                .iter()
                .for_each(|field| field.visit_bindings(bind)),
            Pat::ListExact(items) => items
                .iter()
                .for_each(|item| item.visit_bindings(bind)),
            Pat::ListFront(items, tail) => {
                items
                    .iter()
                    .for_each(|item| item.visit_bindings(bind));
                tail.as_ref().map(|tail| tail.visit_bindings(bind));
            },
            Pat::Variant(_, inner) => inner.visit_bindings(bind),
            Pat::UnionVariant(_, inner) => inner.visit_bindings(bind),
        }
    }

    pub fn binding_names(self: &MirNode<Self>) -> Vec<Local> {
        let mut names = Vec::new();
        self.visit_bindings(&mut |name, _| names.push(name));
        names
    }

    pub fn bindings(self: &MirNode<Self>) -> Vec<(Local, Repr)> {
        let mut names = Vec::new();
        self.visit_bindings(&mut |name, repr| names.push((name, repr.clone())));
        names
    }

    pub fn binds(self: &MirNode<Self>) -> bool {
        let mut binds = false;
        self.visit_bindings(&mut |_, _| binds = true);
        binds
    }

    fn refresh_locals_inner(&mut self, stack: &mut Vec<(Local, Local)>) {
        if let Some(name) = self.name {
            let new_name = stack.iter().rev().find(|(old, _)| *old == name).expect("No such local").1;
            self.name = Some(new_name);
        }
        self.for_children_mut(|expr| expr.refresh_locals_inner(stack));
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GlobalFlags {
    /// Determines whether a global reference may be inlined. By default this is `true`, but inlining is not permitted
    /// for recursive definitions.
    pub can_inline: bool,
}

impl Default for GlobalFlags {
    fn default() -> Self {
        Self {
            can_inline: true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Expr {
    Literal(Literal),
    Local(Local),
    Global(ProcId, Cell<GlobalFlags>),

    Intrinsic(Intrinsic, Vec<MirNode<Self>>),
    Match(MirNode<Self>, Vec<(MirNode<Binding>, MirNode<Self>)>),

    // (captures, arg, body)
    Func(Local, MirNode<Self>),
    Apply(MirNode<Self>, MirNode<Self>),

    Tuple(Vec<MirNode<Self>>),
    Access(MirNode<Self>, usize),
    List(Vec<MirNode<Self>>),

    Variant(usize, MirNode<Self>),
    AccessVariant(MirNode<Self>, usize), // Unsafely assume the value is a specific variant

    Debug(MirNode<Self>),
}

impl Expr {
    pub fn required_globals(&self) -> HashSet<ProcId> {
        let mut globals = HashSet::new();
        self.required_globals_inner(&mut globals);
        globals
    }

    fn required_globals_inner(&self, globals: &mut HashSet<ProcId>) {
        if let Expr::Global(proc, _) = self {
            globals.insert(*proc);
        }

        self.for_children(|expr| expr.required_globals_inner(globals));
    }

    pub fn refresh_locals(&mut self) {
        let required = self.required_locals(None);
        debug_assert_eq!(required.len(), 0, "Cannot refresh locals for an expression\n\n{}\n\nthat captures (required = {:?})", self.print(), required);
        self.refresh_locals_inner(&mut Vec::new());
    }

    fn refresh_locals_inner(&mut self, stack: &mut Vec<(Local, Local)>) {
        match self {
            Expr::Local(local) => {
                let new_local = stack.iter().rev().find(|(old, _)| old == local)
                    .unwrap_or_else(|| panic!("No such local ${} in {:?}", local.0, stack)).1;
                *local = new_local;
            },
            Expr::Match(pred, arms) => {
                pred.refresh_locals_inner(stack);
                for (binding, arm) in arms {
                    let old_stack = stack.len();
                    binding.visit_bindings(&mut |name, _| stack.push((name, Local::new())));

                    binding.refresh_locals_inner(stack);
                    arm.refresh_locals_inner(stack);
                    stack.truncate(old_stack);
                }
            },
            Expr::Func(arg, body) => {
                let new_arg = Local::new();
                stack.push((*arg, new_arg));
                body.refresh_locals_inner(stack);
                stack.pop();
                *arg = new_arg;
            },
            _ => self.for_children_mut(|expr| expr.refresh_locals_inner(stack)),
        }
    }

    fn required_locals_inner(&self, stack: &mut Vec<Local>, required: &mut Vec<Local>) {
        match self {
            Expr::Literal(_) => {},
            Expr::Local(local) => {
                if !stack.contains(local) {
                    required.push(*local);
                }
            },
            Expr::Global(_, _) => {},
            Expr::Intrinsic(_, args) => args
                .iter()
                .for_each(|arg| arg.required_locals_inner(stack, required)),
            Expr::Match(pred, arms) => {
                pred.required_locals_inner(stack, required);
                for (arm, body) in arms {
                    let old_stack = stack.len();
                    stack.append(&mut arm.binding_names());

                    body.required_locals_inner(stack, required);

                    stack.truncate(old_stack);
                }
            },
            Expr::Func(arg, body) => {
                stack.push(*arg);
                body.required_locals_inner(stack, required);
                stack.pop();
            },
            Expr::Apply(f, arg) => {
                f.required_locals_inner(stack, required);
                arg.required_locals_inner(stack, required);
            },
            Expr::Tuple(fields) => fields
                .iter()
                .for_each(|field| field.required_locals_inner(stack, required)),
            Expr::List(items) => items
                .iter()
                .for_each(|item| item.required_locals_inner(stack, required)),
            Expr::Access(tuple, _) => tuple.required_locals_inner(stack, required),
            Expr::Variant(_, inner) => inner.required_locals_inner(stack, required),
            Expr::AccessVariant(inner, _) => inner.required_locals_inner(stack, required),
            Expr::Debug(inner) => inner.required_locals_inner(stack, required),
        }
    }

    pub fn required_locals(&self, already_has: impl IntoIterator<Item = Local>) -> Vec<Local> {
        let mut required = Vec::new();
        self.required_locals_inner(&mut already_has.into_iter().collect(), &mut required);
        required
    }

    pub fn print(&self) -> impl fmt::Display + '_ {
        struct DisplayBinding<'a>(&'a Binding, usize);

        impl<'a> fmt::Display for DisplayBinding<'a> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                if let Some(name) = self.0.name {
                    write!(f, "${}", name.0)?;
                    if let Pat::Wildcard = &self.0.pat {
                        return Ok(());
                    } else {
                        write!(f, " ~ ")?;
                    }
                }
                match &self.0.pat {
                    Pat::Wildcard => write!(f, "_"),
                    Pat::Literal(c) => write!(f, "const {:?}", c),
                    Pat::Single(inner) => write!(f, "{}", DisplayBinding(inner, self.1)),
                    Pat::Variant(variant, inner) => write!(f, "#{} {}", variant, DisplayBinding(inner, self.1)),
                    Pat::UnionVariant(id, inner) => write!(f, "#{} {}", id, DisplayBinding(inner, self.1)),
                    Pat::ListExact(items) => write!(f, "[{}]", items.iter().map(|i| format!("{},", DisplayBinding(i, self.1 + 1))).collect::<Vec<_>>().join(" ")),
                    Pat::ListFront(items, tail) => write!(
                        f,
                        "[{} .. {}]",
                        items.iter().map(|i| format!("{},", DisplayBinding(i, self.1 + 1))).collect::<Vec<_>>().join(" "),
                        tail.as_ref().map(|tail| format!("{}", DisplayBinding(tail, self.1))).unwrap_or_default(),
                    ),
                    Pat::Tuple(fields) => write!(f, "({})", fields.iter().map(|f| format!("{},", DisplayBinding(f, self.1 + 1))).collect::<Vec<_>>().join(" ")),
                    // _ => write!(f, "<PAT>"),
                    pat => todo!("{:?}", pat),
                }
            }
        }

        struct DisplayExpr<'a>(&'a Expr, usize, bool);

        impl<'a> fmt::Display for DisplayExpr<'a> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                use Intrinsic::*;
                if self.2 {
                    write!(f, "{}", "    ".repeat(self.1))?;
                }
                match self.0 {
                    Expr::Local(local) => write!(f, "${}", local.0),
                    Expr::Global(global, _) => write!(f, "global {:?}", global),
                    Expr::Literal(c) => write!(f, "const {:?}", c),
                    Expr::Func(arg, body) => write!(f, "fn ${} =>\n{}", arg.0, DisplayExpr(body, self.1 + 1, true)),
                    Expr::Apply(func, arg) => write!(f, "({})({})", DisplayExpr(func, self.1, false), DisplayExpr(arg, self.1, false)),
                    Expr::Variant(variant, inner) => write!(f, "#{} {}", variant, DisplayExpr(inner, self.1, false)),
                    Expr::Tuple(fields) => write!(f, "({})", fields.iter().map(|f| format!("{},", DisplayExpr(f, self.1 + 1, false))).collect::<Vec<_>>().join(" ")),
                    Expr::List(items) => write!(f, "[{}]", items.iter().map(|i| format!("{}", DisplayExpr(i, self.1 + 1, false))).collect::<Vec<_>>().join(", ")),
                    Expr::Intrinsic(NotBool, args) => write!(f, "!{}", DisplayExpr(&args[0], self.1, false)),
                    Expr::Intrinsic(NegNat | NegInt | NegReal, args) => write!(f, "-{}", DisplayExpr(&args[0], self.1, false)),
                    Expr::Intrinsic(EqChar | EqNat | EqInt, args) => write!(f, "{} = {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(AddNat | AddInt, args) => write!(f, "{} + {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(SubNat | SubInt, args) => write!(f, "{} - {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(MulNat | MulInt, args) => write!(f, "{} * {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(LessNat, args) => write!(f, "{} < {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(MoreNat, args) => write!(f, "{} > {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(MoreEqNat, args) => write!(f, "{} >= {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(LessEqNat, args) => write!(f, "{} <= {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(Join(_), args) => write!(f, "{} ++ {}", DisplayExpr(&args[0], self.1, false), DisplayExpr(&args[1], self.1, false)),
                    Expr::Intrinsic(Union(_), args) => write!(f, "?{}", DisplayExpr(&args[0], self.1, false)),
                    Expr::Match(pred, arms) if arms.len() == 1 => {
                        let (arm, body) = &arms[0];
                        write!(f, "let {} = {} in\n{}", DisplayBinding(arm, self.1 + 1), DisplayExpr(pred, self.1, false), DisplayExpr(body, self.1 + 1, true))
                    },
                    Expr::Match(pred, arms) => {
                        write!(f, "match {} in", DisplayExpr(pred, self.1, false))?;
                        for (arm, body) in arms {
                            write!(f, "\n{}| {} => {}", "    ".repeat(self.1 + 1), DisplayBinding(arm, self.1 + 1), DisplayExpr(body, self.1 + 1, false))?;
                        }
                        if arms.len() == 0 {
                            write!(f, " (no arms)")?;
                        }
                        Ok(())
                    },
                    Expr::Debug(inner) => write!(f, "?{}", DisplayExpr(inner, self.1, false)),
                    // _ => write!(f, "<TODO>"),
                    expr => todo!("{:?}", expr),
                }
            }
        }

        DisplayExpr(self, 0, true)
    }
}
