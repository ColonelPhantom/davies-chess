pub struct LazySort<'a, T, C> {
    data: &'a [T],
    keys: Vec<C>,
    seen: Vec<bool>,
}

impl <'a, T, C: Ord> LazySort<'a, T, C> {
    pub fn new<F>(data: &'a [T], mut key_fn: F) -> Self
    where
        F: FnMut(&T) -> C,
    {
        let keys = data.iter().map(|item| key_fn(item)).collect();
        let seen = vec![false; data.len()];
        Self { data, keys, seen }
    }

    pub fn seen(&self) -> impl Iterator<Item=&'a T> {
        self.data.iter().zip(self.seen.iter()).filter_map(|(item, &s)| if s { Some(item) } else { None })
    }
}

impl<'a, T, C> Iterator for LazySort<'a, T, C> where C: PartialOrd {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let mut best: Option<usize> = None;
        for i in 0..self.data.len() {
            if self.seen[i] {
                continue;
            }
            if let Some(best_idx) = best {
                if self.keys[i] < self.keys[best_idx] {
                    best = Some(i);
                }
            } else {
                best = Some(i);
            }
        }
        if let Some(best_idx) = best {
            self.seen[best_idx] = true;
            Some(&self.data[best_idx])
        } else {
            None
            
        }
    }
}