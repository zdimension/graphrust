use crate::app::Person;

pub mod louvain;

pub trait AbstractNode {
    fn neighbors(&self) -> &Vec<usize>;
}

impl AbstractNode for Person {
    fn neighbors(&self) -> &Vec<usize> {
        &self.neighbors
    }
}

pub trait AbstractGraph<'a> {
    fn get_edges(self) -> impl Iterator<Item=(usize, usize)> + 'a;
}

impl<'a, N: AbstractNode + 'a, G: Iterator<Item=&'a N> + 'a> AbstractGraph<'a> for G {
    fn get_edges(self) -> impl Iterator<Item=(usize, usize)> + 'a {
        self
            .enumerate()
            .flat_map(|(i, n)| {
                n.neighbors().iter()
                    .filter(move |&&j| i < j)
                    .map(move |&j| (i, j))
            })
    }
}
