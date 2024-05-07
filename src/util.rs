pub fn camel_to_snake(camel: &str) -> String {
    let mut snake = String::new();

    for (i, ch) in camel.chars().enumerate() {
        if ch.is_uppercase() && i != 0 {
            snake.push('_');
        }
        snake.push(ch.to_lowercase().next().unwrap());
    }

    snake
}
