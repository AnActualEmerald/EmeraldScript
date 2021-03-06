#[cfg(test)]
mod tests;
mod types;
mod builtins;

use crate::interpreter::types::EmObject;
use crate::interpreter::types::Indexable;

use super::lexer::Expression;
use super::parser::ExprNode;

use std::fmt;
use std::{cell::RefCell, collections::HashMap};

///Represents everything that exists in the language currently
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    Null,
    Float(f32),
    EmString(String),
    EmBool(bool),
    EmArray(Vec<Box<Value>>),
    //Char(u8),
    Name(String),
    Function(Expression, Vec<Value>, ExprNode),
    Object(EmObject),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Float(s) => write!(f, "{}", s),
            Value::EmString(s) => write!(f, "{}", s),
            // Value::Char(c) => write!(f, "{}", c),
            Value::Name(n) => write!(f, "{}", n),
            Value::Null => write!(f, "null"),
            Value::Function(n, p, _) => write!(f, "{:?}({:?})", n, p),
            Value::EmBool(b) => write!(f, "{}", b),
            Value::EmArray(v) => {
                let mut tmp = String::new();
                for val in v.iter() {
                    if let Value::EmString(_) = **val{
                        tmp = format!("{}\"{}\", ", tmp, val);
                    }else {
                        tmp = format!("{}{}, ", tmp, val);

                    }
                }
                tmp.pop();
                tmp.pop();
                write!(f, "[{}]", tmp)
            }
            Value::Object(e) => {
                if let Some(Value::Function(_, _, t)) = e.get_prop("~display") {
                    let mut rt = Runtime::new();
                    let mut gf = StackFrame::new();
                    gf.set_var(String::from("self"), self.clone());
                    let res = repl_run(t.clone(), &mut rt, &mut gf).unwrap_or_default();
                    write!(f, "{}", res)
                } else {
                    write!(f, "{:?}", e.members)
                }
            }
        }
    }
}

impl types::Indexable<Value> for Value {
    fn index<'a>(&'a self, index: usize) -> Result<&'a Value, String> {
        match self {
            Value::EmArray(v) => {
                if let Some(val) = v.get(index) {
                    Ok(val)
                } else {
                    Err(format!(
                        "Index {} out of bounds (I hope I can include line numbers some day)",
                        index
                    ))
                }
            }
            _ => Err(format!("Type {} isn't indexable", self)),
        }
    }

    fn index_mut<'a>(&'a mut self, index: usize) -> Result<&'a mut Value, String> {
        match self {
            Value::EmArray(v) => {
                if let Some(val) = v.get_mut(index) {
                    Ok(val)
                } else {
                    Err(format!(
                        "Index {} out of bounds (I hope I can include line numbers some day)",
                        index
                    ))
                }
            }
            _ => Err(format!("Type {} isn't indexable", self)),
        }
    }
}

///Stores variables in a hashmap for a given function block. Only created on function call, with the exception of the global frame
pub struct StackFrame {
    stack: HashMap<String, Value>,
}

///Handles all of the interpretation, and keeps track of things like function definitions
pub struct Runtime {
    // tree: ExprNode,
    // stack: Vec<StackFrame>,
    heap: HashMap<String, RefCell<Value>>,
    functions: HashMap<String, Box<dyn Fn(Vec<Value>) -> Value>>,
    returning: bool,
}

///A run function that accepts a runtime and global frame, mostly for use with the REPL
pub fn repl_run(
    tree: ExprNode,
    runtime: &mut Runtime,
    glob_frame: &mut StackFrame,
) -> Result<String, String> {
    match runtime.walk_tree(&tree, glob_frame) {
        Ok(val) => Ok(format!("{}", val)),
        Err(e) => Err(e),
    }
}

///Walks through the provided tree and executes all the nodes
pub fn run(tree: ExprNode, args: ExprNode) {
    let mut r = Runtime::new();
    // r.find_global_vars();
    let mut glob_frame = StackFrame::new();

    //define all functions and any global variables
    if let Err(e) = r.walk_tree(&tree, &mut glob_frame) {
        println!("Interpreter crashed because: {}", e);
    }

    if let Err(e) = r.do_call(&Expression::Ident("main".to_owned()), &[args], &mut glob_frame) {
        println!("Interpreter crashed because: {}", e);
    }
    // println!("{:?}", glob_frame.stack);
}

// Basically *is* the interpreter, walks through the AST and executes the nodes as needed
impl Runtime {
    //TODO: Reduce the number of copies ins this code

    ///Creates a new Runtime with an empty heap
    pub fn new() -> Runtime {
        Runtime {
            heap: HashMap::new(),
            returning: false,
            functions: builtins::get_functions(),
        }
    }

    ///Matches the provided node and dispatches functions to handle it
    fn walk_tree(&mut self, node: &ExprNode, frame: &mut StackFrame) -> Result<Value, String> {
        // println!(
        //     "Walking tree: \n    Current node: {:?}\n     Current stack: {:?}",
        //     node, frame.stack
        // );
        let res: Value;
        match node {
            ExprNode::Block(v) => {
                let mut ret = Value::Null;
                for e in v.iter() {
                    match e {
                        /*When we run into a ReturnVal, it needs special treatment so we know to stop executing the
                         *current block once we get whatever the value is
                         **/
                        ExprNode::ReturnVal(v) => {
                            ret = self.walk_tree(v, frame)?;
                            break;
                        }
                        _ => {
                            self.walk_tree(e, frame)?;
                        }
                    }
                    if self.returning {
                        //if the returning flag has been set, then break out of the loop and stop executing this block
                        //This is for return statements that don't return anything
                        break;
                    }
                }
                return Ok(ret);
            }
            ExprNode::Operation(o, l, r) => res = self.do_operation(&**o, &**l, &**r, frame)?,
            ExprNode::Call(ex, n) => res = self.do_call(&**ex, &*n, frame)?,
            ExprNode::MethodCall(n, args) => res = self.do_method(n, args, frame)?,
            ExprNode::StrLiteral(s) => res = Value::EmString(*s.clone()),
            ExprNode::NumLiteral(n) => res = Value::Float(**n),
            ExprNode::BoolLiteral(b) => res = Value::EmBool(*b),
            ExprNode::Name(n) => res = frame.get_var_copy(n),
            ExprNode::Func(n, p, b) => res = self.def_func(n, p, b)?, //don't need the stackframe here because functions are stored on the heap
            ExprNode::Statement(e) => res = self.walk_tree(&**e, frame)?,
            ExprNode::Loop(ty, con, block) => res = self.do_loop(&**ty, &**con, &**block, frame)?,
            ExprNode::IfStatement(con, body, branch) => {
                res = self.do_if(con, body, branch, frame)?
            }
            ExprNode::Array(v) => res = self.create_array(v, frame)?,
            ExprNode::Index(ident, index) => res = self.index_array(ident, index, frame)?,
            ExprNode::New(name, args) => res = self.do_init(name, args, frame)?,
            ExprNode::Class(name, body) => res = self.define_class(&**name, &**body, frame)?,
            _ => res = Value::Null,
        }
        //Reset the returning flag, since we're returning whatever value we got anyways
        self.returning = false;
        Ok(res)
    }

    ///Executes both varieties of loop and walks through the nodes in the loop blocks
    fn do_loop(
        &mut self,
        ty: &str,
        condition: &ExprNode,
        block: &ExprNode,
        frame: &mut StackFrame,
    ) -> Result<Value, String> {
        match ty {
            "while" => {
                let mut ret = Value::Null;
                // println!("Loop condition: {:?}\nBlock to run: {:?}", condition, block);
                // println!(
                //     "Condition is currently: {:?}",
                //     self.walk_tree(&condition, frame)
                // );
                while self.walk_tree(&condition, frame)? == Value::EmBool(true) {
                    ret = self.walk_tree(&block, frame)?;
                    if self.returning {
                        break;
                    }
                }
                Ok(ret)
            }
            "for" => {
                let mut ret = Value::Null;
                if let ExprNode::ForLoopDec(dec, con, inc) = condition {
                    if let ExprNode::Illegal(_) = **dec {
                        while self.walk_tree(&con, frame)? == Value::EmBool(true) {
                            //walk the tree to execute the loop body
                            ret = self.walk_tree(&block, frame)?;
                            if self.returning {
                                break;
                            }
                            //perform the incrementation
                            self.walk_tree(&inc, frame)?;
                        }
                    } else {
                        self.walk_tree(&dec, frame)?;
                        while self.walk_tree(&con, frame)? == Value::EmBool(true) {
                            //walk the tree to execute the loop body
                            ret = self.walk_tree(&block, frame)?;
                            if self.returning {
                                break;
                            }
                            //perform the incrementation
                            self.walk_tree(&inc, frame)?;
                        }
                    }
                }

                Ok(ret)
            }
            _ => Ok(Value::Null),
        }
    }

    ///Defines a function and saves it as a variable in the heap
    fn def_func(
        &mut self,
        name: &Expression,
        params: &[ExprNode],
        body: &ExprNode,
    ) -> Result<Value, String> {
        if let Expression::Ident(n) = name {
            let mut args = vec![];
            params.iter().for_each(|e| {
                if let ExprNode::Name(n) = e {
                    args.push(Value::Name(n.to_string()));
                }
            });
            let f = Value::Function(name.clone(), args, body.clone());
            self.heap.insert(n.to_owned(), RefCell::new(f.clone()));
            Ok(f)
        } else {
            Err(format!("Expected identifier, found {:?}", name))
            //If we don't get a name for the funciton, we should exit since things will break
        }
    }

    ///Performs arithmatic and boolean operations and returns their results
    fn do_operation(
        &mut self,
        opr: &Expression,
        left: &ExprNode,
        right: &ExprNode,
        frame: &mut StackFrame,
    ) -> Result<Value, String> {
        match opr {
            Expression::Equal => match left {
                ExprNode::Name(n) => {
                    let v = self.walk_tree(&right, frame)?;
                    // println!("Assigning variable: {:?}", v);
                    frame.set_var(n.to_string(), v.clone());
                    Ok(v)
                }
                ExprNode::Index(n, i) => {
                    let name = if let ExprNode::Name(s) = *n.clone() {
                        *s
                    } else {
                        return Err(format!("Error getting name {:?}", n));
                    };
                    let index = self.walk_tree(i, frame)?;
                    let val = self.walk_tree(right, frame)?;
                    frame.update_array_index(&name, index, val.clone());

                    Ok(val)
                }
                ExprNode::Operation(o, l, r) => {
                    match **o {
                        Expression::Lbracket => {
                            let val = self.walk_tree(right, frame)?;
                            frame.update_nested_array(l, r, Some(val.clone()), true);
                            Ok(val)
                        }
                        Expression::Operator(op) => {
                            match op {
                                '.' => {
                                    let name = if let ExprNode::Name(n) = &**l {
                                        *n.clone()
                                    }else{
                                        return Err(format!("Expected name, got {:?}", l));
                                    };
                                    let val = self.walk_tree(right, frame)?;

                                    if let Some(Value::Object(e)) = frame.get_var_mut(&name.to_string()){
                                        let prop = if let ExprNode::Name(n) = &**r {
                                            n
                                        }else {
                                            return Err(format!("Unexpected symbol {:?}", r));
                                        };

                                        e.set_prop(*prop.clone(), Box::new(val.clone()));
                                        Ok(val)
                                    }else {
                                        Err(format!("Unexpected {:?}", name))
                                    }
                                }
                                _ => Err(format!("Unexpected operator {}", op))
                            }
                        }
                        _ => Err(format!("Unexpected symbol {:?}", o))
                    }
                }
                _ => Err(format!("Error assigning to variable {:?}", left)),
            },

            Expression::Operator(o) => {
                if *o == '.' {
                    // let val = self.walk_tree(&left, frame)?;
                    return if let Value::Object(obj) = self.walk_tree(&left, frame)? {
                        if let Some(v) = obj.get_prop(&right.inner()) {
                            Ok(v.clone())
                        }else {
                            Err(format!("{} has no property {}", obj, right.inner()))
                        }
                    } else {
                        Err(format!("{:?} is not an object", left))
                    }

                }
                let l_p = self.walk_tree(&left, frame)?;
                let r_p = self.walk_tree(&right, frame)?;

               

                let f = match l_p {
                    Value::Float(f) => f,
                    Value::Name(n) => {
                        if let Value::Float(f) = frame.get_var(&n) {
                            *f
                        } else {
                            0.0 as f32
                        }
                    }
                    Value::EmString(s) => {
                        return Ok(Value::EmString(format!("{}{}", s, r_p)))
                    },
                    _ => 0.0 as f32,
                };

                let r = match r_p {
                    Value::Float(f) => f,
                    Value::Name(n) => {
                        if let Value::Float(f) = frame.get_var(&n) {
                            *f
                        } else {
                            0.0 as f32
                        }
                    }
                    _ => 0.0 as f32,
                };

                if *o == '+' {
                    Ok(Value::Float(f + r))
                } else if *o == '-' {
                    Ok(Value::Float(f - r))
                } else if *o == '*' {
                    Ok(Value::Float(f * r))
                } else if *o == '/' {
                    Ok(Value::Float(f / r))
                } else {
                    Err(format!("Invalid Operator: {}", o))
                }
            }
            Expression::BoolOp(op) => {
                let l_p = self.walk_tree(&left, frame)?;
                let r_p = self.walk_tree(&right, frame)?;
                match op.as_str() {
                    "==" => Ok(Value::EmBool(l_p == r_p)),
                    "!=" => Ok(Value::EmBool(l_p != r_p)),
                    ">=" => Ok(Value::EmBool(l_p >= r_p)),
                    "<=" => Ok(Value::EmBool(l_p <= r_p)),
                    "<" => Ok(Value::EmBool(l_p < r_p)),
                    ">" => Ok(Value::EmBool(l_p > r_p)),
                    _ => Err(format!("Invalid Operator: {}", op)),
                }
            }

            Expression::Lbracket => Ok(self.index_array(left, right, frame)?),
            _ => Ok(Value::Null),
        }
    }

    fn keyword(
        &mut self,
        name: &Expression,
        value: &ExprNode,
        frame: &mut StackFrame,
    ) -> Result<Value, String> {
        if let Expression::Key(s) = name { 
            let tmp = match value {
                        ExprNode::Call(n, args) => 
                            self.do_call(n, args, frame)?,
                        
                        _ => self.walk_tree(&value, frame)?,
                        };
            match s.as_str() {
                "return" => {
                    self.returning = true;
                    return Ok(tmp);
                }
                _ => {
                   
                }
            }
        }

        Ok(Value::Null)
    }

    ///Executes a keyword or function call
    fn do_call(
        &mut self,
        name: &Expression,
        args: &[ExprNode],
        frame: &mut StackFrame,
    ) -> Result<Value, String> {
        match name {
            Expression::Key(_) => self.keyword(name, &args[0], frame),
            Expression::Ident(n) => {
                //check if there is a built-in function to use
                if self.functions.contains_key(n) {
                    let tmp = args.iter()
                    .map(|e| self.walk_tree(e, frame).unwrap())
                    .collect();
                    let func = self.functions.get(n).unwrap();
                    return Ok(func(tmp))
                }

                if let Some(func) = self.heap.get(n) {
                    //I'd really like to not have to borrow here
                    match &*func.clone().borrow() {
                        Value::Function(_, params, body) => {
                            if params.len() != args.len() {
                                Err(format!(
                                    "Expected {} arguments for {}, got {}",
                                    params.len(),
                                    n,
                                    args.len()
                                ))
                            } else {
                                let mut func_frame = StackFrame::new();
                                for (i, e) in args.iter().enumerate() {
                                    if let Value::Name(arg) = &params[i] {
                                        let val = self.walk_tree(&e, frame)?;
                                        match val {
                                            Value::Name(n) => {
                                                let tmp = frame.get_var(&n).clone();
                                                func_frame.set_var(arg.to_string(), tmp);
                                                //I'd really like to not have to copy here
                                            }
                                            _ => func_frame.set_var(arg.to_string(), val),
                                        }
                                    }
                                }
                                self.walk_tree(&body, &mut func_frame)
                                //this shouldn't be necessary since Rust will destroy the old
                                //stack frame anyways when it goes out of  scope
                                // params.iter().for_each(|e| {
                                //     if let Value::Name(n) = e {
                                //         frame.free_var(n)
                                //     }
                                // });
                            }
                        }
                        _ => Err(format!("Expected function, found {}", func.borrow())),
                    }
                } else {
                    Err(format!("Couldn't find identifier {}", n))
                }
            }
            _ => Err(format!("Expected keyword or identifier, found {:?}", name)),
        }
    }

    fn do_method(&mut self, method: &ExprNode, args: &Vec<ExprNode>, frame: &mut StackFrame) -> Result<Value, String> {
        if let ExprNode::Operation(_, name, member) = method {
            if let Value::Object(e) = self.walk_tree(&**name, frame)?{
                let func = e.get_prop(&*member.inner());
                match func {
                    Some(Value::Function(n, p, body)) => {
                        if args.len() != p.len() - 1 {
                            Err(format!(
                                "Method {} for {} takes {} arguments, found {}",
                                n,
                                e.get_prop("~name").unwrap(),
                                p.len(),
                                args.len()
                            ))
                        } else {
                            let mut func_frame = StackFrame::new();
                            func_frame.set_var(String::from("self"), Value::Object(e.clone()));
                            for (i, e) in args.iter().enumerate() {
                                if let Value::Name(arg) = &p[i+1] {
                                    let val = self.walk_tree(&e, frame)?;
                                    match val {
                                        Value::Name(n) => {
                                            let tmp = frame.get_var(&n).clone();
                                            func_frame.set_var(arg.to_string(), tmp);
                                            //I'd really like to not have to copy here
                                        }
                                        _ => func_frame.set_var(arg.to_string(), val),
                                    }
                                }
                            }
                            self.walk_tree(body, &mut func_frame)
                        }
                    }
                    _ => {
                        Err(format!("Expected function, got {:?}", func))
                    }
                }
            }else {
                Err(format!("Expected object, got {:?}", self.walk_tree(name, frame)?))
            }
        } else {
            Err(format!("Unexpected expression {:?}", method))
        }
    }
    ///Performs an if statement and any of its relevant branches
    fn do_if(
        &mut self,
        condition: &ExprNode,
        body: &ExprNode,
        branches: &ExprNode,
        frame: &mut StackFrame,
    ) -> Result<Value, String> {
        if self.walk_tree(condition, frame)? == Value::EmBool(true) {
            self.walk_tree(body, frame)
        } else if let ExprNode::IfStatement(con, body, branch) = branches {
            self.do_if(con, body, branch, frame)
        } else {
            self.walk_tree(branches, frame)
        }
    }



    fn do_init(
        &mut self,
        name: &Expression,
        init_args: &Vec<ExprNode>,
        frame: &mut StackFrame,
    ) -> Result<Value, String> {
        if let Expression::Ident(n) = name{
            let class = match self.heap.get(n) {
                Some(val) => {
                    if let Value::Object(e) = val.borrow().clone(){
                        e
                    }else {
                        return Err(format!("Expected class, got {}", val.borrow()));
                    }
                },
                None => return Err(format!("Class {} is not defined", name)),

            };
        if let Some(Value::Function(_, params, body)) = class.get_prop("~init") {
            if init_args.len() != params.len() - 1 {
                Err(format!(
                    "Contrsuctor for {} takes {} arguments, found {}",
                    class.get_prop("~name").unwrap(),
                    params.len(),
                    init_args.len()
                ))
            } else {
                let mut func_frame = StackFrame::new();
                func_frame.set_var(String::from("self"), Value::Object(class.clone()));
                for (i, e) in init_args.iter().enumerate() {
                    if let Value::Name(arg) = &params[i+1] {
                        let val = self.walk_tree(&e, frame)?;
                        match val {
                            Value::Name(n) => {
                                let tmp = frame.get_var(&n).clone();
                                func_frame.set_var(arg.to_string(), tmp);
                                //I'd really like to not have to copy here
                            }
                            _ => func_frame.set_var(arg.to_string(), val),
                        }
                    }
                }
                self.walk_tree(body, &mut func_frame)?;
                
                //should figure out a way to get ownership from a stackframe
                Ok(func_frame.get_var("self").clone())
            }
        } else {
            Ok(Value::Object(class))
        }
    }else {
        Err(format!("Expected object, found {:?}", name))
    }
    }

    ///Defines an array and saves it to the current stackframe
    fn create_array(
        &mut self,
        raw: &Vec<ExprNode>,
        frame: &mut StackFrame,
    ) -> Result<Value, String> {
        let mut tmp = vec![];
        for val in raw.iter() {
            tmp.push(Box::new(self.walk_tree(val, frame)?));
        }

        Ok(Value::EmArray(tmp))
    }

    ///Returns the value at a given array index
    fn index_array(
        &mut self,
        ident: &ExprNode,
        index: &ExprNode,
        frame: &mut StackFrame,
    ) -> Result<Value, String> {
        let array = self.walk_tree(ident, frame)?;
        if let Value::Float(f) = self.walk_tree(index, frame)? {
            Ok(array.index(f as usize)?.clone())
        } else {
            Err(format!("Index was not a numeber"))
        }
    }

    fn define_class(&mut self, name: &Expression, body: &ExprNode, frame: &mut StackFrame) -> Result<Value, String> {
        let mut members = HashMap::new();
        let class = if let Expression::Ident(s) = name{
            s
        }else {
            return Err("Expected an identifier".to_string());
        }; 

        //the name property will be the name of the class for now, this might change in the future
        members.insert("~name".to_string(), Box::new(Value::EmString(class.clone())));

        if let ExprNode::Block(v) = body {
            for node in v {
                let val = self.walk_tree(node, frame)?;
                match &val {
                    Value::Function(n, _, _) => {
                        let fn_name = if let Expression::Ident(s) =  n{
                            s
                        }else {
                            return Err("Expected identifier".to_owned());
                        };
                        members.insert(fn_name.clone(), Box::new(val.clone()));
                    }
                    er => {
                        return Err(format!("Unexpected {:?} in class definition", er));
                    }
                }
            }
        }

        let tmp = Value::Object(EmObject {members: members});
        self.heap.insert(class.clone(), RefCell::new(tmp.clone()));

        Ok(tmp)
    }
}

///Keeps track of local variables for functions. Currently only created when a function is called
impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for StackFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl StackFrame {
    pub fn new() -> StackFrame {
        StackFrame {
            stack: HashMap::new(),
        }
    }

    fn set_var(&mut self, name: String, v: Value) {
        self.stack.insert(name, v);
    }

    fn get_var(&self, name: &str) -> &Value {
        if self.stack.contains_key(name) {
            &self.stack[name]
        } else {
            &Value::Null
        }
    }

    fn get_var_mut(&mut self, name: &str) -> Option<&mut Value>{
        if self.stack.contains_key(name){
            self.stack.get_mut(name)
        }else{
            None
        }
    }

    fn update_array_index(&mut self, name: &str, index: Value, val: Value) {
        let var = self
            .stack
            .get_mut(name)
            .expect(format!("Unable to find variable {}", name).as_str());

        if let Value::Float(f) = index {
            match var {
                Value::EmArray(v) => {
                    v[f as usize] = Box::new(val);
                }
                _ => panic!("Expected array, found {}", var),
            }
        }
    }

    fn update_nested_array(
        &mut self,
        ident: &ExprNode,
        index: &ExprNode,
        val: Option<Value>,
        first: bool,
    ) -> Option<&mut Box<Value>> {
        match ident {
            ExprNode::Operation(o, l, r) => {
                if **o != Expression::Lbracket {
                    panic!("Found operation when assigning to array: {:?}", ident);
                } else {
                    let var = self.update_nested_array(l, r, None, false)?;
                    let i = match index {
                        ExprNode::NumLiteral(f) => **f as usize,
                        _ => panic!("Expected number literal, found {:?}", index),
                    };
                    match &mut **var {
                        Value::EmArray(v) => {
                            if first {
                                v[i] = Box::new(val.unwrap());
                                None
                            } else {
                                v.get_mut(i)
                            }
                        }
                        n => panic!("Expected array, found {}", n),
                    }
                }
            }
            ExprNode::Name(n) => {
                let i = match index {
                    ExprNode::NumLiteral(f) => **f as usize,
                    _ => panic!("Expected number literal, found {:?}", index),
                };

                let var = self
                    .stack
                    .get_mut(&**n)
                    .expect(format!("Unable to find variable {}", n).as_str());

                match var {
                    Value::EmArray(v) => v.get_mut(i),
                    _ => panic!("Expected array, found {}", var),
                }
            }

            _ => panic!("Unexpected node: {:?}", ident),
        }
    }

    fn get_var_copy(&self, name: &str) -> Value {
        if self.stack.contains_key(name) {
            self.stack[name].clone()
        } else {
            Value::Null
        }
    }

    //leaving this here for now in case I need it in the future
    // fn free_var(&mut self, name: &str) {
    //     if self.stack.contains_key(name) {
    //         self.stack.remove(name);
    //     }
    // }
}
