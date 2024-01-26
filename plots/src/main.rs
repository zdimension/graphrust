use ahash::AHashMap;
use graph_format::{GraphFile, Readable};
use inline_python::python;
use std::collections::hash_map::Entry::{Occupied, Vacant};

fn main() {
    let file = GraphFile::read_from_file("graph_n4j.bin").unwrap();
    let mut degrees = AHashMap::new();
    let persons = file.get_adjacency();
    for pers in persons {
        match degrees.entry(pers.len()) {
            Occupied(mut e) => {
                *e.get_mut() += 1;
            }
            Vacant(e) => {
                e.insert(1);
            }
        }
    }
    let mut degrees_vec: Vec<_> = degrees.into_iter().collect();
    degrees_vec.sort_by_key(|(k, _)| *k);

    python! {
        import matplotlib.pyplot as plt
        import numpy as np
        import math
        from scipy.optimize import curve_fit

        def power(x, b, c):
            return b*x**c

        mind, maxd = 3, 80
        dat = np.array('degrees_vec)[mind:maxd]

        plt.title("Friend count distribution among {} Facebook users".format(np.sum(dat[:, 1])))

        # find power law
        popt, pcov = curve_fit(power, dat[:, 0], dat[:, 1], p0=[1, -1])
        print(popt)

        # plot log y as bar chart
        plt.bar(dat[:, 0], dat[:, 1])
        plt.xlabel("Friend count")
        plt.ylabel("User count")

        # plot power law
        x = np.linspace(mind, maxd, 100)
        plt.plot(x, power(x, *popt), "r-", label="fit: %5.3f*friends^%5.3f" % tuple(popt))

        plt.legend()

        # set y range to dat
        plt.ylim([1, np.max(dat[:, 1])])

        if False:
            plt.xscale("log")
            plt.yscale("log")

        plt.show()
    }
}
