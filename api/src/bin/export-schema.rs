use microticket::graphql;

fn main() {
    let schema = graphql::build_schema();
    print!("{}", schema.sdl());
}
