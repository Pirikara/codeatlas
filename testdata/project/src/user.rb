class UserService
  def create(name, password)
    User.new(name, password)
  end

  def find(id)
    User.find_by_id(id)
  end
end

class User
  attr_reader :name, :password

  def initialize(name, password)
    @name = name
    @password = password
  end

  def full_name
    name.capitalize
  end

  def self.find_by_id(id)
    # stub
  end
end
