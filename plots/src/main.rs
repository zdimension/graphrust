use ahash::AHashMap;
use graph_format::{GraphFile, Readable};
use inline_python::python;
use std::collections::hash_map::Entry::{Occupied, Vacant};

fn main() {
    let file = GraphFile::read_from_file("graph_n4j.bin").unwrap();
    let mut degrees = AHashMap::new();
    let persons = file.get_adjacency();
    println!(
        "Average degree: {}",
        persons.iter().map(|p| p.len()).sum::<usize>() as f64 / persons.len() as f64
    );
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

        mind, maxd = 7, 80
        dat = np.array('degrees_vec)[mind:maxd]
        deg_x = dat[:, 0]
        total_users = np.sum(dat[:, 1])
        deg_y = np.array(dat[:, 1], dtype=np.float64) / float(total_users)

        plt.title("Friend count distribution among {} Facebook users".format(total_users))

        # find power law
        popt, pcov = curve_fit(power, deg_x, deg_y, p0=[1, -1])
        print(popt)

        # plot log y as bar chart
        plt.bar(deg_x, deg_y)
        plt.xlabel("Friend count")
        plt.ylabel("User count")

        # plot power law
        x = np.linspace(mind, maxd, 100)
        plt.plot(x, power(x, *popt), "r-", label="fit: %5.3f*friends^%5.3f" % tuple(popt))

        plt.legend()

        # set y range to dat

        logarithmic = True

        if logarithmic:
            plt.xscale("log")
            plt.yscale("log")
            plt.ylim([1e-7, 1])
        else:
            plt.ylim([0, np.max(deg_y)])

        plt.show()
    }
}
