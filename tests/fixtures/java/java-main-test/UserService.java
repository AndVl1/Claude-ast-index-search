import java.util.List;

public interface UserRepository {
    User findById(Long id);
    User save(User user);
}

public class UserService implements UserRepository {
    @Override
    public User findById(Long id) {
        return new User(id, "Alice", "alice@example.com");
    }

    @Override
    public User save(User user) {
        return user;
    }

    public List<User> findAll() {
        return List.of();
    }
}
