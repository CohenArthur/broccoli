//! A `Construct` is a complex set of tokens. For example, `fn()` is an identifier, a
//! left parenthesis and a right parenthesis. Together, they constitute a function call.
//! In the same vein, `x = 12;` is 4 tokens used to represent variable assignment.
//! Therefore, constructs use tokens while the parser only uses constructs. This is an
//! abstraction for all possible ways to parse a line in jinko.
//!
//! Each of the functions in that module contain the grammar they represent above their
//! name. The syntax used for the grammar is loosely based on regular instructions and
//! globbing. One can use * to indicate 0 or more, ? to indicate 1 or more, etc etc.
//! Optional parameters are included between brackets. For example,
//!
//! `[mut] <identifier> = <const> | <function_call> | <block> | <identifier>`
//!
//! is the grammar for a variable assignment.

use nom::{branch::alt, combinator::opt, combinator::peek, multi::many0, IResult};

use crate::instruction::{
    Audit, Block, DecArg, FunctionCall, FunctionDec, FunctionKind, IfElse, Incl, Instruction,
    JkInst, Loop, LoopKind, MethodCall, Return, TypeDec, TypeInstantiation, Var, VarAssign,
};
use crate::parser::{BoxConstruct, ConstantConstruct, ShuntingYard, Token};

type ParseResult<'i, T> = IResult<&'i str, T>;

pub struct Construct;

impl Construct {
    /// Parse any valid jinko instruction. This can be a function call, a variable,
    /// a block declaration...
    pub fn instruction(input: &str) -> ParseResult<Box<dyn Instruction>> {
        let (input, _) = Token::maybe_consume_extra(input)?;

        // FIXME: If input is empty, return an error or do nothing
        // FIXME: We need to parse the remaining input after a correct instruction
        // has been parsed
        let (input, value) = alt((
            Construct::binary_op,
            BoxConstruct::method_call,
            BoxConstruct::function_declaration,
            BoxConstruct::type_declaration,
            BoxConstruct::ext_declaration,
            BoxConstruct::test_declaration,
            BoxConstruct::mock_declaration,
            BoxConstruct::type_instantiation,
            BoxConstruct::function_call,
            BoxConstruct::incl,
            BoxConstruct::if_else,
            BoxConstruct::any_loop,
            BoxConstruct::jinko_inst,
            BoxConstruct::audit,
            BoxConstruct::block,
            BoxConstruct::var_assignment,
            BoxConstruct::variable,
            BoxConstruct::return_expression,
            Construct::constant,
        ))(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;

        Ok((input, value))
    }

    /// Parse an instruction and maybe the semicolon that follows.
    ///
    /// `<instruction> [ ; ]`
    pub fn instruction_maybe_semicolon(input: &str) -> ParseResult<Box<dyn Instruction>> {
        let (input, expr) = Construct::instruction(input)?;
        let (input, _) = opt(Token::semicolon)(input)?;

        Ok((input, expr))
    }

    /// Parse as many instructions as possible
    pub fn many_instructions(input: &str) -> ParseResult<Vec<Box<dyn Instruction>>> {
        many0(Construct::instruction_maybe_semicolon)(input)
    }

    /// Constants are raw values in the source code. For example, `"string"`, `12` and
    /// `0.5`.
    ///
    /// `'<any_char>' | "<any_char>*" | <num>? | <num>?.<num>?`
    pub(crate) fn constant(input: &str) -> ParseResult<Box<dyn Instruction>> {
        alt((
            ConstantConstruct::char_constant,
            ConstantConstruct::string_constant,
            ConstantConstruct::float_constant,
            ConstantConstruct::int_constant,
            ConstantConstruct::bool_constant,
        ))(input)
    }

    /// Parse a function call with no arguments
    ///
    /// `<identifier> ( )`
    fn function_call_no_args(input: &str) -> ParseResult<FunctionCall> {
        let (input, fn_id) = Token::identifier(input)?;
        let (input, _) = Token::left_parenthesis(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::right_parenthesis(input)?;

        Ok((input, FunctionCall::new(fn_id.to_owned())))
    }

    /// Parse an argument given to a function. Consumes the whitespaces before and after
    /// the argument
    fn arg(input: &str) -> ParseResult<Box<dyn Instruction>> {
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, constant) = Construct::instruction(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;

        Ok((input, constant))
    }

    /// Parse an argument and the comma that follows it
    fn arg_and_comma(input: &str) -> ParseResult<Box<dyn Instruction>> {
        let (input, constant) = Construct::instruction(input)?;
        let (input, _) = Token::comma(input)?;

        Ok((input, constant))
    }

    /// Parse a list of arguments separated by comma
    fn args_list(input: &str) -> ParseResult<Vec<Box<dyn Instruction>>> {
        // Get 1 or more arguments with a comma to the function call
        let (input, mut arg_vec) = many0(Construct::arg_and_comma)(input)?;

        // Parse the last argument, which does not have a comma. There needs to be
        // at least one argument, which can be this one
        let (input, last_arg) = Construct::arg(input)?;

        arg_vec.push(last_arg);

        Ok((input, arg_vec))
    }

    /// Parse a function call with arguments
    fn function_call_args(input: &str) -> ParseResult<FunctionCall> {
        let (input, fn_id) = Token::identifier(input)?;
        let (input, _) = Token::left_parenthesis(input)?;

        let mut fn_call = FunctionCall::new(fn_id.to_owned());

        let (input, mut arg_vec) = Construct::args_list(input)?;
        let (input, _) = Token::right_parenthesis(input)?;

        arg_vec.drain(0..).for_each(|arg| fn_call.add_arg(arg));

        Ok((input, fn_call))
    }

    /// When a type is instantiated in the source code.
    ///
    /// ```
    /// type A(n: int); // Declare type A
    /// val = A(1); // Instantiate a new A type variable
    /// ```
    /// `<arg_list> := [(<constant> | <variable> | <expression>)*]`
    /// `<identifier> ( <arg_list> )`
    pub fn type_instantiation(input: &str) -> ParseResult<TypeInstantiation> {
        let (input, type_id) = Token::identifier(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::left_curly_bracket(input)?;

        let mut type_instantiation = TypeInstantiation::new(type_id.to_owned());

        let (input, mut arg_vec) = Construct::args_list(input)?;
        let (input, _) = Token::right_curly_bracket(input)?;

        arg_vec
            .drain(0..)
            .for_each(|field| type_instantiation.add_field(field));

        Ok((input, type_instantiation))
    }

    /// When a function is called in the source code.
    ///
    /// ```
    /// fn(); // Function call
    /// fn() // Call the function `fn` and use the return result as an instruction
    /// x = fn(); // Assign the result of the function call to the variable x
    /// ```
    ///
    /// `<arg_list> := [(<constant> | <variable> | <instruction>)*]`
    /// `<identifier> ( <arg_list> )`
    pub(crate) fn function_call(input: &str) -> ParseResult<FunctionCall> {
        alt((
            Construct::function_call_no_args,
            Construct::function_call_args,
        ))(input)
    }

    /// When a variable is assigned a value. Ideally, a variable cannot be assigned the
    /// `void` type.
    ///
    /// ```
    /// x = 12; // Store 12 into the variable `x`
    /// x = 456; // Forbidden, `x` is immutable
    /// mut n = 12; // Store 12 into `n`, a mutable variable
    /// n = 1586; // Allowed
    /// ```
    ///
    /// A variable assignment is a Statement. It cannot be used as an Expression
    ///
    /// ```
    /// {
    ///     x = 12; // Block returns void
    /// }
    /// {
    ///     x = 12 // Forbidden
    /// }
    /// {
    ///     x = call();
    ///     x // Okay
    /// } // But it's easier to just...
    /// {
    ///     call()
    /// }
    /// ```
    ///
    /// `[mut] <identifier> = ( <constant> | <function_call> ) ;`
    pub(crate) fn var_assignment(input: &str) -> ParseResult<VarAssign> {
        let (input, mut_opt) = opt(Token::mut_tok)(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, id) = Token::identifier(input)?;
        let (input, _) = opt(Token::consume_whitespaces)(input)?;
        let (input, _) = Token::equal(input)?;
        let (input, _) = opt(Token::consume_whitespaces)(input)?;
        let (input, value) = Construct::instruction(input)?;

        match mut_opt {
            Some(_) => Ok((input, VarAssign::new(true, id.to_owned(), value))),
            None => Ok((input, VarAssign::new(false, id.to_owned(), value))),
        }
    }

    /// Parse a valid variable name
    ///
    /// `<identifier>`
    pub(crate) fn variable(input: &str) -> ParseResult<Var> {
        let (input, name) = Token::identifier(input)?;

        Ok((input, Var::new(name.to_owned())))
    }

    /// Parse a statement and the semicolon that follows
    ///
    /// `<instruction> ;`
    fn stmt_semicolon(input: &str) -> ParseResult<Box<dyn Instruction>> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, expr) = Construct::instruction(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::semicolon(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        Ok((input, expr))
    }

    /// Parse multiple statements and a possible return Instruction
    fn stmts_and_maybe_last(
        input: &str,
    ) -> ParseResult<(Vec<Box<dyn Instruction>>, Option<Box<dyn Instruction>>)> {
        let (input, instructions) = many0(Construct::stmt_semicolon)(input)?;
        let (input, last_expr) = opt(Construct::instruction)(input)?;

        Ok((input, (instructions, last_expr)))
    }

    /// Parses the statements in a block as well as a possible last instruction
    fn block_instructions(
        input: &str,
    ) -> ParseResult<(Vec<Box<dyn Instruction>>, Option<Box<dyn Instruction>>)> {
        let (input, _) = Token::left_curly_bracket(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, (instructions, last)) = Construct::stmts_and_maybe_last(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::right_curly_bracket(input)?;

        Ok((input, (instructions, last)))
    }

    /// A block of code is a new inner scope that contains instructions. You can use
    /// them in If/Else blocks, in function declarations, or just as is.
    ///
    /// ```
    /// func return_nothing() {
    ///     compute_stuff();
    /// } // Block returns void, so does the function
    ///
    /// x = {
    ///     12
    /// } // Block returns 12
    ///
    /// x = {
    ///     compute_stuff();
    ///     some_other_stuff();
    ///     12
    /// } // Block returns 12 after having called two functions
    /// ```
    ///
    /// There can only be one returning instruction, and it must be the last one
    /// in the block.
    ///
    /// `{ [ <instruction> ; ]* [ <instruction> ] }`
    pub(crate) fn block(input: &str) -> ParseResult<Block> {
        let (input, (instructions, last)) = Construct::block_instructions(input)?;

        let mut block = Block::new();
        block.set_instructions(instructions);
        block.set_last(last);

        Ok((input, block))
    }

    /// Parse an empty argument declaration list
    ///
    /// `( )`
    fn args_dec_empty(input: &str) -> ParseResult<Vec<DecArg>> {
        let (input, _) = Token::left_parenthesis(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::right_parenthesis(input)?;

        Ok((input, vec![]))
    }

    /// Parse an identifier then its type
    ///
    /// `<identifier> : <type>`
    fn identifier_type(input: &str) -> ParseResult<DecArg> {
        let (input, id) = Token::identifier(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::colon(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, ty) = Token::identifier(input)?;

        Ok((input, DecArg::new(id.to_owned(), ty.to_owned())))
    }

    /// Parse an identifer as well as the type and comma that follows
    ///
    /// `<identifer> : <type> ,`
    fn identifier_type_comma(input: &str) -> ParseResult<DecArg> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, arg) = Construct::identifier_type(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::comma(input)?;

        Ok((input, arg))
    }

    /// Parse a non empty argument declaration list
    ///
    /// `( [ <identifier> : <type> ]* )`
    fn args_dec_non_empty(input: &str) -> ParseResult<Vec<DecArg>> {
        let (input, _) = Token::left_parenthesis(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, mut args) = many0(Construct::identifier_type_comma)(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        // Parse the last argument which does not have a comma
        let (input, last_arg) = Construct::identifier_type(input)?;
        args.push(last_arg);

        let (input, _) = Token::right_parenthesis(input)?;

        Ok((input, args))
    }

    /// Parse a list (maybe empty) of argument declarations
    fn args_dec(input: &str) -> ParseResult<Vec<DecArg>> {
        alt((Construct::args_dec_empty, Construct::args_dec_non_empty))(input)
    }

    /// Parse the void return type of a function, checking that no arrow is present
    fn return_type_void(input: &str) -> ParseResult<Option<String>> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, arrow) = opt(Token::arrow)(input)?;

        match arrow {
            Some(_) => Err(nom::Err::Error((input, nom::error::ErrorKind::OneOf))),
            None => Ok((input, None)),
        }
    }

    /// Parse a non-void return type
    fn return_type_non_void(input: &str) -> ParseResult<Option<String>> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::arrow(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, ty) = Token::identifier(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        Ok((input, Some(ty.to_owned())))
    }

    /// Parse the return type of a function. Can be void
    fn return_type(input: &str) -> ParseResult<Option<String>> {
        alt((Construct::return_type_non_void, Construct::return_type_void))(input)
    }

    /// Parses the content of a function declaration
    ///
    /// `<identifier> <args_dec> <return_type> <block>`
    fn function_content(input: &str) -> ParseResult<FunctionDec> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, fn_name) = Token::identifier(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, args) = Construct::args_dec(input)?;
        let (input, ty) = Construct::return_type(input)?;
        let (input, block) = Construct::block(input)?;

        let mut function = FunctionDec::new(fn_name.to_owned(), ty);

        function.set_args(args);
        function.set_block(block);

        Ok((input, function))
    }

    /// Parse a function declaration. This includes the function's signature and the
    /// associated code block
    ///
    /// ```
    /// func fn_name(arg0: int) -> int {
    ///     do_something(arg0);
    ///
    ///     12
    /// }
    /// ```
    ///
    /// `<typed_arg_list> := [ (<identifier> : <type>)* ]
    /// `<func> <identifier> ( <typed_arg_list> ) [ -> <type> ] <block>`
    pub(crate) fn function_declaration(input: &str) -> ParseResult<FunctionDec> {
        let (input, _) = Token::func_tok(input)?;

        let (input, mut function) = Construct::function_content(input)?;
        function.set_kind(FunctionKind::Func);

        Ok((input, function))
    }

    /// Parse a test declaration. This returns a FunctionDec as well, but of
    /// kind `FunctionDec::Test`.
    /// test functions are non-callable by the programmer. Only the interpreter can
    /// invoke them. Therefore, naming the test the same as the tested function is fine
    /// and is not any form of overloading whatsoever.
    ///
    /// ```
    /// test add() {
    ///     assert_eq(12 + 2, add(12, 2));
    /// }
    /// ```
    ///
    /// `<test> <identifier> ( ) <block>
    pub(crate) fn test_declaration(input: &str) -> ParseResult<FunctionDec> {
        let (input, _) = Token::test_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, fn_name) = Token::identifier(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, args) = Construct::args_dec(input)?;
        let (input, ty) = Construct::return_type_void(input)?;
        let (input, block) = Construct::block(input)?;

        let mut function = FunctionDec::new(fn_name.to_owned(), ty);

        function.set_args(args);
        function.set_block(block);

        function.set_kind(FunctionKind::Test);

        Ok((input, function))
    }

    /// Parse a mock declaration. This returns a FunctionDec as well, but of
    /// kind `FunctionDec::Mock`.
    ///
    ///
    /// ```
    /// mock add(lhs: int, rhs: int) -> int {
    ///     mock_stuff()
    /// }
    /// ```
    ///
    /// `<mock> <identifier> ( <typed_arg_list> ) [ -> <type> ] <block>
    pub(crate) fn mock_declaration(input: &str) -> ParseResult<FunctionDec> {
        let (input, _) = Token::mock_tok(input)?;

        let (input, mut function) = Construct::function_content(input)?;
        function.set_kind(FunctionKind::Mock);

        Ok((input, function))
    }

    /// Parse an external function declaration.
    ///
    /// External functions cannot have an associated block. The function's code resides
    /// in a native program, for example a shared C library or a Rust crate.
    ///
    /// `<ext> <func> <identifier> ( <typed_arg_list> ) [ -> <type> ] ;`
    pub(crate) fn ext_declaration(input: &str) -> ParseResult<FunctionDec> {
        let (input, _) = Token::ext_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::func_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, fn_name) = Token::identifier(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, args) = Construct::args_dec(input)?;
        let (input, ty) = Construct::return_type(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::semicolon(input)?;

        let mut function = FunctionDec::new(fn_name.to_owned(), ty);

        function.set_args(args);

        function.set_kind(FunctionKind::Ext);

        Ok((input, function))
    }

    /// Parse an `else` plus the associated block
    fn else_block(input: &str) -> ParseResult<Block> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::else_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        Construct::block(input)
    }

    /// Parse an if/else construct. This parses the entirety of the if/else. Therefore
    /// consuming the first `if` and the remaining optional `else`.
    ///
    /// `<if> <block> [ <else> <block> ]`
    pub(crate) fn if_else(input: &str) -> ParseResult<IfElse> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::if_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, condition) = Construct::instruction(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, if_body) = Construct::block(input)?;

        let (input, else_body) = opt(Construct::else_block)(input)?;

        let if_else = IfElse::new(condition, if_body, else_body);

        Ok((input, if_else))
    }

    /// Parse an audit block. This consists in the the audit keyword and the following
    /// block. Audit blocks are useful to relax the interpreter and develop faster. For
    /// example, you're allowed to ignore return values in an audit block.
    ///
    /// `<audit> <block>`
    pub(crate) fn audit(input: &str) -> ParseResult<Audit> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::audit_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, block) = Construct::block(input)?;

        Ok((input, Audit::new(block)))
    }

    /// Parse a loop block, meaning the `loop` keyword and a corresponding block
    ///
    /// `<loop> <block>`
    fn loop_block(input: &str) -> ParseResult<Loop> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::loop_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, block) = Construct::block(input)?;

        Ok((input, Loop::new(LoopKind::Loop, block)))
    }

    /// Parse a while block. A while block consists of a high bound, or instruction, as
    /// well as a block
    ///
    /// `<while> <instruction> <block>`
    fn while_block(input: &str) -> ParseResult<Loop> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::while_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, condition) = Construct::instruction(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, block) = Construct::block(input)?;

        Ok((input, Loop::new(LoopKind::While(condition), block)))
    }

    /// Construct a for block, which consists of a variable, a range instruction, and
    /// a block to execute
    ///
    /// `<for> <variable> <in> <instruction> <block>`
    fn for_block(input: &str) -> ParseResult<Loop> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::for_tok(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, variable) = Construct::variable(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::in_tok(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, instruction) = Construct::instruction(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, block) = Construct::block(input)?;

        Ok((
            input,
            Loop::new(LoopKind::For(variable, instruction), block),
        ))
    }

    /// Parse any loop construct: For, While or Loop
    pub(crate) fn any_loop(input: &str) -> ParseResult<Loop> {
        alt((
            Construct::loop_block,
            Construct::for_block,
            Construct::while_block,
        ))(input)
    }

    /// Parse an interpreter directive. There are only a few of them, listed in
    /// the `JkInst` module
    ///
    /// `@<jinko_inst><args_list>`
    pub(crate) fn jinko_inst(input: &str) -> ParseResult<JkInst> {
        let (input, _) = Token::at_sign(input)?;
        let (input, fc) = Construct::function_call(input)?;

        // FIXME: No unwrap(), use something else than just the name
        let inst = JkInst::from_str(fc.name()).unwrap();

        Ok((input, inst))
    }

    /// Parse a binary operation. A binary operation is composed of an instruction, an
    /// operator and another instruction
    ///
    /// `<expr> <op> <expr>`
    ///
    /// ```
    /// x + y; // Add x and y together
    /// a << 2; // Shift a by 2 bits
    /// a > 2; // Is a greater than 2?
    /// ```
    pub(crate) fn binary_op(input: &str) -> ParseResult<Box<dyn Instruction>> {
        ShuntingYard::parse(input)
    }

    /// Parse a user-defined custom type
    ///
    /// `<type> <TypeName> ( <typed_arg_list> ) ;`
    pub(crate) fn type_declaration(input: &str) -> ParseResult<TypeDec> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, _) = Token::_type_tok(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, type_name) = Token::identifier(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, fields) = Construct::args_dec_non_empty(input)?;

        let type_declaration = TypeDec::new(type_name.to_owned(), fields);

        Ok((input, type_declaration))
    }

    /// Parses a path for code inclusion
    fn path(input: &str) -> ParseResult<String> {
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, path) = Token::identifier(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;

        Ok((input, path.to_string()))
    }

    fn as_identifier(input: &str) -> ParseResult<Option<String>> {
        let (input, _) = Token::maybe_consume_extra(input)?;
        let (input, id) = match opt(Token::as_tok)(input)? {
            (input, Some(_)) => {
                let (input, _) = Token::maybe_consume_extra(input)?;
                let (input, id) = Token::identifier(input)?;

                (input, Some(id.to_string()))
            }
            (input, None) => (input, None),
        };

        let (input, _) = Token::maybe_consume_extra(input)?;

        Ok((input, id))
    }

    /// Parse an include statement and its possible aliasing
    ///
    /// `<incl> <path> [ <as> <alias> ]
    pub(crate) fn incl(input: &str) -> ParseResult<Incl> {
        let (input, _) = Token::maybe_consume_extra(input)?;

        let (input, _) = Token::incl_tok(input)?;
        let (input, path) = Construct::path(input)?;

        let (input, rename) = Construct::as_identifier(input)?;

        let (input, _) = Token::maybe_consume_extra(input)?;

        let incl = Incl::new(path, rename);

        Ok((input, incl))
    }

    /// Parse a viable caller for a method call
    fn method_caller(input: &str) -> ParseResult<Box<dyn Instruction>> {
        // FIXME: Right now, we cannot chain method calls and no error is produced:
        // `1.double().double()` returns 2 instead of the expected 4, since
        // only one call is resolved and the remaining input (`.double()`) is
        // silently ignored
        alt((
            BoxConstruct::function_call,
            BoxConstruct::variable,
            Construct::constant,
            BoxConstruct::if_else,
            BoxConstruct::block,
            BoxConstruct::any_loop,
            BoxConstruct::jinko_inst,
            BoxConstruct::audit,
        ))(input)
    }

    /// Parse a method like function call, that shall be desugared
    /// to a simple function call later on
    ///
    /// `<identifier>.<identifier>()`
    pub fn method_call(input: &str) -> ParseResult<MethodCall> {
        let (input, caller) = Construct::method_caller(input)?;
        let (input, _) = Token::dot(input)?;
        let (input, method) = Construct::function_call(input)?;

        Ok((input, MethodCall::new(caller, method)))
    }

    pub fn return_expression(input: &str) -> ParseResult<Return> {
        // println!("Return start");
        let (input, _) = Token::return_tok(input)?;
        // println!("Return tok -> {}", input);

        let (input, _) = Token::maybe_consume_extra(input)?;
        // println!("After white space -> {}", input);

        let (input, ret_val) = opt(Construct::instruction)(input)?;
        let (input, _) = Token::maybe_consume_extra(input)?;
        println!("Input is \"{}\"", input);
        if input != "" {
            // There is still something
            return Err(nom::Err::Error((input, nom::error::ErrorKind::NonEmpty)));
        }

        Ok((input, Return::new(ret_val)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_var_assign_valid() {
        assert_eq!(
            Construct::var_assignment("x = 12;").unwrap().1.mutable(),
            false
        );
        assert_eq!(
            Construct::var_assignment("x = 12;").unwrap().1.symbol(),
            "x"
        );

        assert_eq!(
            Construct::var_assignment("mut x_99 = 129;")
                .unwrap()
                .1
                .mutable(),
            true
        );
        assert_eq!(
            Construct::var_assignment("mut x_99 = 129;")
                .unwrap()
                .1
                .symbol(),
            "x_99"
        );

        assert_eq!(
            Construct::var_assignment("mut_x_99 = 129;")
                .unwrap()
                .1
                .mutable(),
            false
        );
        assert_eq!(
            Construct::var_assignment("mut_x_99 = 129;")
                .unwrap()
                .1
                .symbol(),
            "mut_x_99"
        );

        assert_eq!(
            Construct::var_assignment("mut mut_x_99 = 129;")
                .unwrap()
                .1
                .mutable(),
            true
        );
        assert_eq!(
            Construct::var_assignment("mut mut_x_99 = 129;")
                .unwrap()
                .1
                .symbol(),
            "mut_x_99"
        );

        assert_eq!(
            Construct::var_assignment("mut\nname = 129;")
                .unwrap()
                .1
                .mutable(),
            true
        );

        match Construct::var_assignment("mut x=12;") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Equal stuck to id is allowed"),
        }
        match Construct::var_assignment("mut x= 12;") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Equal stuck to id is allowed"),
        }
        match Construct::var_assignment("mut x =12;") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Equal stuck to value is allowed"),
        }
    }

    #[test]
    fn t_var_assign_invalid() {
        match Construct::var_assignment("mutable x = 12") {
            Ok(_) => assert!(false, "Mutable isn't mut"),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn t_function_call_no_args_valid() {
        assert_eq!(Construct::function_call("fn()").unwrap().1.name(), "fn");
        assert_eq!(Construct::function_call("fn()").unwrap().1.args().len(), 0);

        assert_eq!(Construct::function_call("fn(    )").unwrap().1.name(), "fn");
        assert_eq!(
            Construct::function_call("fn(    )").unwrap().1.args().len(),
            0
        );
    }

    #[test]
    fn t_function_call_valid() {
        assert_eq!(Construct::function_call("fn(2)").unwrap().1.name(), "fn");
        assert_eq!(Construct::function_call("fn(2)").unwrap().1.args().len(), 1);

        assert_eq!(
            Construct::function_call("fn(1, 2, 3)").unwrap().1.name(),
            "fn"
        );
        assert_eq!(
            Construct::function_call("fn(a, hey(), 3.12)")
                .unwrap()
                .1
                .name(),
            "fn"
        );
        assert_eq!(
            Construct::function_call("fn(1, 2, 3)")
                .unwrap()
                .1
                .args()
                .len(),
            3
        );

        assert_eq!(
            Construct::function_call("fn(1   , 2,3)").unwrap().1.name(),
            "fn"
        );
        assert_eq!(
            Construct::function_call("fn(1   , 2,3)")
                .unwrap()
                .1
                .args()
                .len(),
            3
        );
    }

    #[test]
    fn t_function_call_invalid() {
        match Construct::function_call("fn(") {
            Ok(_) => assert!(false, "Unterminated parenthesis"),
            Err(_) => assert!(true),
        }
        match Construct::function_call("fn))") {
            Ok(_) => assert!(false, "Wrong parenthesis"),
            Err(_) => assert!(true),
        }
        match Construct::function_call("fn((") {
            Ok(_) => assert!(false, "Wrong parenthesis again"),
            Err(_) => assert!(true),
        }
        match Construct::function_call("fn((") {
            Ok(_) => assert!(false, "Wrong parenthesis again"),
            Err(_) => assert!(true),
        }
        match Construct::function_call("fn((") {
            Ok(_) => assert!(false, "Wrong parenthesis again"),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn t_function_call_multiarg_invalid() {
        match Construct::function_call("fn(1, 2, 3, 4,)") {
            Ok(_) => assert!(false, "Unterminated arglist"),
            Err(_) => assert!(true),
        }
        match Construct::function_call("fn(1, 2, 3, 4,   )") {
            Ok(_) => assert!(false, "Unterminated arglist"),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn t_block_empty() {
        assert_eq!(Construct::block("{}").unwrap().1.instructions().len(), 0);
    }

    #[test]
    fn t_block_valid_oneline() {
        assert_eq!(
            Construct::block("{ 12a; }").unwrap().1.instructions().len(),
            1
        );
        assert_eq!(
            Construct::block("{ 12a; 14a; }")
                .unwrap()
                .1
                .instructions()
                .len(),
            2
        );
        assert_eq!(
            Construct::block("{ 12a; 14a }")
                .unwrap()
                .1
                .instructions()
                .len(),
            1
        );

        match Construct::block("{ 12a; 14a }").unwrap().1.last() {
            Some(_) => assert!(true),
            None => assert!(false, "Last instruction here is valid"),
        }
    }

    #[test]
    fn t_id_type_valid() {
        assert_eq!(
            Construct::identifier_type("name: some_type")
                .unwrap()
                .1
                .name(),
            "name"
        );
        assert_eq!(
            Construct::identifier_type("name: some_type")
                .unwrap()
                .1
                .ty(),
            "some_type"
        );

        assert_eq!(
            Construct::identifier_type("name     :some_type")
                .unwrap()
                .1
                .name(),
            "name"
        );
        assert_eq!(
            Construct::identifier_type("name     :some_type")
                .unwrap()
                .1
                .ty(),
            "some_type"
        );
    }

    #[test]
    fn t_args_dec_empty() {
        assert_eq!(Construct::args_dec("()").unwrap().1.len(), 0);
    }

    #[test]
    fn t_args_dec_one_arg() {
        assert_eq!(Construct::args_dec("(name :ty)").unwrap().1.len(), 1);
    }

    #[test]
    fn t_args_dec_valid() {
        assert_eq!(
            Construct::args_dec("(name :ty, name1      : type1)")
                .unwrap()
                .1
                .len(),
            2
        );
    }

    #[test]
    fn t_return_type_void() {
        assert_eq!(Construct::return_type(""), Ok(("", None)));
        assert_eq!(Construct::return_type("    "), Ok(("", None)));
        assert_eq!(
            Construct::return_type("        { 12 }"),
            Ok(("{ 12 }", None))
        );
    }

    #[test]
    fn t_block_invalid_oneline() {
        match Construct::block("{ 12a;") {
            Ok(_) => assert!(false, "Unterminated bracket"),
            Err(_) => assert!(true),
        }

        match Construct::block("{ 12a") {
            Ok(_) => assert!(false, "Unterminated bracket but on instruction"),
            Err(_) => assert!(true),
        }

        match Construct::block("{ 12a; 13a") {
            Ok(_) => assert!(false, "Unterminated bracket but on second instruction"),
            Err(_) => assert!(true),
        }

        match Construct::block("12a; 13a }") {
            Ok(_) => assert!(false, "Not starting with a bracket"),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn t_block_valid_multiline() {
        let input = r#"{
                12a;
                12a;
                13a;
            }"#;

        assert_eq!(Construct::block(input).unwrap().1.instructions().len(), 3);

        let input = r#"{
                12a;
                12a;
                13a;
                14a
            }"#;

        assert_eq!(Construct::block(input).unwrap().1.instructions().len(), 3);

        let input = r#"{
                true;
                false
            }"#;

        assert_eq!(Construct::block(input).unwrap().1.instructions().len(), 1);
    }

    #[test]
    fn t_return_type_non_void() {
        assert_eq!(
            Construct::return_type("-> int"),
            Ok(("", Some("int".to_owned())))
        );
        assert_eq!(
            Construct::return_type("   ->    int   {"),
            Ok(("{", Some("int".to_owned())))
        );
    }

    #[test]
    fn t_function_declaration_valid_simple() {
        let func = Construct::function_declaration("func something() {}")
            .unwrap()
            .1;

        assert_eq!(func.name(), "something");
        assert_eq!(func.ty(), None);
        assert_eq!(func.args().len(), 0);
        assert_eq!(func.fn_kind(), FunctionKind::Func);
    }

    #[test]
    fn t_function_declaration_valid() {
        let func = Construct::function_declaration("func add(lhs: ty, rhs: ty) -> ty {}")
            .unwrap()
            .1;

        assert_eq!(func.name(), "add");
        assert_eq!(func.ty(), Some(&"ty".to_owned()));
        assert_eq!(func.args().len(), 2);
        assert_eq!(func.fn_kind(), FunctionKind::Func);
    }

    #[test]
    fn t_test_valid() {
        let test = Construct::test_declaration("test add() {}").unwrap().1;

        assert_eq!(test.name(), "add");
        assert_eq!(test.ty(), None);
        assert_eq!(test.fn_kind(), FunctionKind::Test);
    }

    #[test]
    fn t_test_invalid() {
        match Construct::test_declaration("test add(a: int) -> int {}") {
            Ok(_) => assert!(false, "Can't have arguments to a test"),
            Err(_) => assert!(true),
        };
    }

    #[test]
    fn t_mock_valid() {
        let test = Construct::mock_declaration("mock add(lhs: ty, rhs: ty) {}")
            .unwrap()
            .1;

        assert_eq!(test.name(), "add");
        assert_eq!(test.ty(), None);
        assert_eq!(test.fn_kind(), FunctionKind::Mock);
    }

    #[test]
    fn t_ext_valid() {
        let test = Construct::ext_declaration("ext func add(lhs: ty, rhs: ty) -> ty;")
            .unwrap()
            .1;

        assert_eq!(test.name(), "add");
        assert_eq!(test.ty(), Some(&"ty".to_owned()));
        assert_eq!(test.fn_kind(), FunctionKind::Ext);
    }

    #[test]
    fn t_ext_valid_void() {
        let test = Construct::ext_declaration("ext func add(lhs: ty, rhs: ty);")
            .unwrap()
            .1;

        assert_eq!(test.name(), "add");
        assert_eq!(test.ty(), None);
        assert_eq!(test.fn_kind(), FunctionKind::Ext);
    }

    #[test]
    fn t_ext_invalid() {
        match Construct::ext_declaration("ext func add(a: int) -> int {}") {
            Ok(_) => assert!(false, "Can't have a block for an ext function"),
            Err(_) => assert!(true),
        };
    }

    #[test]
    fn t_if_else_just_if() {
        let ie = Construct::if_else("if condition {}");

        match &ie {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Valid to only have if"),
        };
    }

    #[test]
    fn t_if_else() {
        match Construct::if_else("if condition {} else {}") {
            Ok((input, _)) => assert_eq!(input, ""),
            Err(_) => assert!(false, "Valid to have empty blocks"),
        };
    }

    #[test]
    fn t_audit_simple() {
        match Construct::audit("audit {}") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Valid audit syntax"),
        }
    }

    #[test]
    fn t_loop_valid() {
        match Construct::loop_block("loop {}") {
            Ok((i, _)) => assert_eq!(i, ""),
            Err(_) => assert!(false, "Valid empty loop"),
        }
    }

    #[test]
    fn t_loop_invalid() {
        match Construct::loop_block("loo {}") {
            Ok(_) => assert!(false, "`loo` is not the keyword"),
            Err(_) => assert!(true),
        };

        match Construct::loop_block("loop") {
            Ok(_) => assert!(false, "A block is required"),
            Err(_) => assert!(true),
        };
    }

    #[test]
    fn t_while_valid() {
        match Construct::while_block("while x_99 {}") {
            Ok((i, _)) => assert_eq!(i, ""),
            Err(_) => assert!(false, "Valid empty while"),
        }
    }

    #[test]
    fn t_while_invalid() {
        match Construct::while_block("while {}") {
            Ok(_) => assert!(false, "Need a condition"),
            Err(_) => assert!(true),
        };

        match Construct::while_block("while") {
            Ok(_) => assert!(false, "A block is required"),
            Err(_) => assert!(true),
        };
    }

    #[test]
    fn t_for_valid() {
        match Construct::for_block("for x_99 in x_99 {}") {
            Ok((i, _)) => assert_eq!(i, ""),
            Err(_) => assert!(false, "Valid empty for"),
        }
    }

    #[test]
    fn t_for_invalid() {
        match Construct::for_block("for {}") {
            Ok(_) => assert!(false, "Need a variable and range"),
            Err(_) => assert!(true),
        };

        match Construct::for_block("for x99 in {}") {
            Ok(_) => assert!(false, "A range is required"),
            Err(_) => assert!(true),
        };

        match Construct::for_block("for x99 in { { { inner_block() } } }") {
            Ok(_) => assert!(false, "A range is required"),
            Err(_) => assert!(true),
        };
    }

    #[test]
    fn t_jinko_inst_valid() {
        assert_eq!(Construct::jinko_inst("@dump()"), Ok(("", JkInst::Dump)));
        assert_eq!(
            Construct::jinko_inst("@quit(something, something_else)"),
            Ok(("", JkInst::Quit))
        );
    }

    #[test]
    fn t_binary_op_valid() {
        match Construct::binary_op("a *   12 ") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Valid to have multi spaces"),
        };
        match Construct::binary_op("some() + 12.1") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Valid to have multiple instruction types"),
        };
    }

    #[test]
    fn t_binary_op_invalid() {
        match Construct::binary_op("a ? 12") {
            Ok(_) => assert!(false, "? is not a binop"),
            Err(_) => assert!(true),
        };
    }

    #[test]
    fn t_type_declaration_simple() {
        match Construct::type_declaration("type Int(v: int);") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Just one int is valid"),
        };
        match Construct::type_declaration("type Ints(a: int, b: int);") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Two integers is valid"),
        };
        match Construct::type_declaration("type Compound(i: int, s: str);") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Different types are valid"),
        };
        match Construct::type_declaration(
            "type Custom(v: int, a: SomeType, b: Another, c: lower_case);",
        ) {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Custom types in custom types are valid"),
        };
    }

    #[test]
    fn t_type_declaration_empty() {
        match Construct::type_declaration("type Empty();") {
            Ok(_) => assert!(false, "Can't have empty types"),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn t_type_declaration_invalid() {
        match Construct::type_declaration("type ExtraComma(a: int, b: int,);") {
            Ok(_) => assert!(false, "Extra comma in type definition"),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn t_type_instantiation_valid() {
        match Construct::type_instantiation("Custom { 1 }") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Type instantiation with one value is valid"),
        }
    }

    #[test]
    fn t_type_instantiation_invalid() {
        match Construct::type_instantiation("Custom { ") {
            Ok(_) => assert!(false),
            Err(_) => assert!(true, "Type instantiation need a closing brace"),
        }
    }

    #[test]
    fn t_type_instantiation_no_name() {
        match Construct::type_instantiation("{ 1 }") {
            Ok(_) => assert!(false),
            Err(_) => assert!(true, "Type instantiation need a type name"),
        }
    }

    #[test]
    fn t_func_dec_binop() {
        match Construct::function_declaration("func a(a: int, b:int) -> int { a + b }") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Valid to directly return a binop"),
        }
    }

    #[test]
    fn t_func_dec_return_arg() {
        match Construct::function_declaration("func a(a: int, b:int) -> int { a }") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Valid to directly return an argument"),
        }
    }

    #[test]
    fn t_func_dec_return_arg_plus_stmt() {
        match Construct::function_declaration("func a(a: int, b:int) -> int { something(); a }") {
            Ok(_) => assert!(true),
            Err(_) => assert!(
                false,
                "Valid to directly return an argument after a statement"
            ),
        }
    }

    #[test]
    fn t_func_dec_return_binop_plus_stmt() {
        match Construct::function_declaration("func a(a: int, b:int) -> int { something(); a + b }")
        {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Valid to directly return a binop after a statement"),
        }
    }

    #[test]
    fn t_func_dec_return_binop_as_var() {
        match Construct::function_declaration("func a(a: int, b:int) -> int { res = a + b; res }") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "Valid to directly return a binop as a variable"),
        }
    }

    #[test]
    fn t_func_call_with_true_arg_is_func_call() {
        let res = Construct::instruction("h(true)").unwrap().1;
        res.downcast_ref::<FunctionCall>().unwrap();

        // There might be a bug that a function call with just a boolean argument gets
        // parsed as a variable. This test aims at correcting that regression. If it
        // fails, then it means the function call did not get parsed as a function call
    }

    #[test]
    fn t_incl_valid() {
        match Construct::incl("incl simple") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "`incl simple` is perfectly valid"),
        }
    }

    #[test]
    fn t_incl_valid_plus_rename() {
        match Construct::incl("incl a as b") {
            Ok(_) => assert!(true),
            Err(_) => assert!(false, "incl + renaming has to be handled"),
        }
    }

    #[test]
    fn t_incl_invalid() {
        match Construct::incl("incl") {
            Ok(_) => assert!(false, "Can't include nothing"),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn t_incl_plus_rename_invalid() {
        match Construct::incl("incl a as") {
            Ok(_) => assert!(false, "Can't include a and rename as nothing"),
            Err(_) => assert!(true),
        }
    }

    #[test]
    fn t_method_call_simple() {
        assert!(
            Construct::method_call("a.b()").is_ok(),
            "Valid to have simple identifiers"
        );
        assert!(
            Construct::method_call("135.method()").is_ok(),
            "Valid to have constant as caller"
        );
        assert!(
            Construct::method_call("{ hey }.method()").is_ok(),
            "Valid to have block as caller"
        );
        assert!(
            Construct::method_call("func_call().method()").is_ok(),
            "Valid to have call as caller"
        );
    }

    #[test]
    fn t_method_call_invalid() {
        assert!(
            Construct::method_call("a.b").is_err(),
            "Missing parentheses"
        );
        assert!(
            Construct::method_call("a.()").is_err(),
            "Missing method name"
        );
        assert!(
            Construct::method_call(".method()").is_err(),
            "Missing caller"
        );
    }

    #[test]
    fn t_return_valid() {
        assert!(
            Construct::return_expression("return").is_ok(),
            "Empty return is valid"
        );
        assert!(
            Construct::return_expression("return 1").is_ok(),
            "Returning a constant is allowed"
        );
    }

    #[test]
    fn t_return_invalid() {
        assert!(
            Construct::return_expression("return 1 2").is_err(),
            "Returning multiple values is not allowed"
        );

        assert!(
            Construct::return_expression("return return").is_err(),
            "Returning a return is not allowed"
        );
    }
}
